#![forbid(unsafe_code)]

//! Action lifecycle management for the AegisAI Runtime control loop.
//!
//! The current implementation is intentionally conservative: it models bounded
//! user-space actions with lease tracking, audit metadata, and deterministic
//! rollback on expiry. Real system call execution can be layered on top later.

mod actuator;
mod backend;
mod cgroup_isolation;
pub mod cpu_affinity;
mod cpuset_dry_run;
mod model;

pub use actuator::Actuator;
pub use backend::{
    ActuatorBackend, BackendApplyResult, BackendExecution, BackendLease, BackendOperation,
    BackendOperationKind, CommandLinuxSyscallApplier, DisabledWarmupExecutorRunner,
    DryRunLinuxCommandRunner, DryRunWarmupExecutorRunner, LinuxActuatorBackend, LinuxAffinityState,
    LinuxCapturedState, LinuxCommandRunner, LinuxCpusetState, LinuxNiceState,
    LinuxProcessStateProvider, LinuxRollbackReport, LinuxSyscallApplier, LinuxSyscallExecutor,
    LinuxSyscallOperation, LinuxSyscallPhase, LinuxSyscallPlan, LiveLinuxCommandGuard,
    NoopActuatorBackend, PlannedLinuxSyscallApplier, PlannedOnlyLinuxSyscallExecutor,
    ProcfsLinuxProcessStateProvider, RecordingActuatorBackend, SystemLinuxCommandRunner,
    SystemWarmupExecutorRunner, UnavailableLinuxProcessStateProvider,
    UnconfirmedLinuxCommandRunner, WarmupExecutorCommand, WarmupExecutorRunner,
};
pub use cgroup_isolation::{
    CgroupFs, OwnedCgroupIsolationApplier, OwnedCgroupIsolationApplyResult,
    OwnedCgroupIsolationGuard, OwnedCgroupIsolationLease, OwnedCgroupIsolationRequest,
    SystemCgroupFs,
};
pub use cpu_affinity::{CpuAffinityCapture, CpuAffinityPlanner, CpuAffinityTarget, CpuTopology};
pub use cpuset_dry_run::{
    plan_cpuset_dry_run, CpusetCapturePlan, CpusetDryRunMode, CpusetDryRunPlan,
    CpusetDryRunRejection, CpusetDryRunRejectionReason, CpusetDryRunRequest,
    CpusetDryRunTargetContext, CpusetProcessClassification, CpusetProcessTarget,
    CpusetRollbackCapture, CpusetRollbackPlan,
};
pub use model::{Action, ActionPlan, AppliedAction, AppliedActionState, PinStrategy, ScenarioKind};

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::collections::BTreeMap;
    use std::rc::Rc;

    use crate::{
        Action, ActionPlan, Actuator, AppliedActionState, CommandLinuxSyscallApplier,
        LinuxActuatorBackend, LinuxAffinityState, LinuxCommandRunner, LinuxCpusetState,
        LinuxNiceState, LinuxProcessStateProvider, LinuxSyscallApplier, LinuxSyscallOperation,
        LiveLinuxCommandGuard, NoopActuatorBackend, PinStrategy, PlannedOnlyLinuxSyscallExecutor,
        RecordingActuatorBackend, ScenarioKind, UnavailableLinuxProcessStateProvider,
        WarmupExecutorCommand, WarmupExecutorRunner,
    };

    fn sample_plan() -> ActionPlan {
        let mut audit_fields = BTreeMap::new();
        audit_fields.insert("source".to_string(), "policy_engine".to_string());

        ActionPlan {
            scenario: ScenarioKind::InferenceTailGuard,
            target_pid: 42,
            target_process_name: "ollama".to_string(),
            actions: vec![
                Action::RaiseNice { delta: -5 },
                Action::SetAffinity {
                    strategy: PinStrategy::PreferReservedCores,
                    max_cpu_ratio: 0.5,
                },
            ],
            duration_ms: 800,
            rationale: vec!["run queue delay breached".to_string()],
            audit_fields,
        }
    }

    fn sample_plan_with_disabled_cpuset() -> ActionPlan {
        let mut plan = sample_plan();
        plan.actions.push(Action::UseCpuset { enabled: false });
        plan
    }

    #[test]
    fn tracks_revertible_actions_until_lease_expiry() {
        let mut actuator = Actuator::default();
        let applied = actuator.apply(sample_plan(), 1_000, true);

        assert_eq!(applied.state, AppliedActionState::Applied);
        assert_eq!(applied.applied_at_ms, 1_000);
        assert_eq!(applied.expires_at_ms, 1_800);
        assert_eq!(actuator.active_count(), 1);

        assert!(actuator.expire(1_799).is_empty());

        let rollbacks = actuator.expire(1_800);
        assert_eq!(rollbacks.len(), 1);
        assert_eq!(rollbacks[0].state, AppliedActionState::RolledBack);
        assert_eq!(rollbacks[0].target_pid, 42);
        assert_eq!(actuator.active_count(), 0);
    }

    #[test]
    fn non_revertible_actions_are_not_tracked() {
        let mut actuator = Actuator::default();
        let applied = actuator.apply(sample_plan(), 5_000, false);

        assert_eq!(applied.state, AppliedActionState::Applied);
        assert_eq!(applied.expires_at_ms, 5_000);
        assert_eq!(actuator.active_count(), 0);
        assert!(actuator.expire(5_001).is_empty());
    }

    #[test]
    fn reapplying_same_pid_and_scenario_refreshes_active_lease() {
        let mut actuator = Actuator::default();
        actuator.apply(sample_plan(), 10_000, true);

        let mut refreshed = sample_plan();
        refreshed.duration_ms = 400;
        refreshed.actions = vec![Action::WarmupExecutor];

        let applied = actuator.apply(refreshed, 10_300, true);

        assert_eq!(applied.actions, vec![Action::WarmupExecutor]);
        assert_eq!(applied.expires_at_ms, 10_700);
        assert_eq!(actuator.active_count(), 1);
        assert!(actuator.expire(10_699).is_empty());
        assert_eq!(actuator.expire(10_700).len(), 1);
    }

    #[test]
    fn reapplying_same_pid_and_scenario_rolls_back_only_refreshed_lease() {
        let mut actuator = Actuator::with_backend(RecordingActuatorBackend::default());
        actuator.apply(sample_plan(), 10_000, true);

        let mut refreshed = sample_plan();
        refreshed.duration_ms = 1_000;
        refreshed.actions = vec![Action::WarmupExecutor];
        actuator.apply(refreshed, 10_300, true);

        assert!(actuator.expire(10_800).is_empty());

        let rollbacks = actuator.expire(11_300);
        assert_eq!(rollbacks.len(), 1);
        assert_eq!(rollbacks[0].applied_at_ms, 10_300);
        assert_eq!(rollbacks[0].actions, vec![Action::WarmupExecutor]);
        assert_eq!(
            rollbacks[0]
                .audit_fields
                .get("backend.rollback.operation_index"),
            Some(&"3".to_string())
        );
    }

    #[test]
    fn expire_returns_due_actions_in_stable_deadline_order() {
        let mut actuator = Actuator::with_backend(RecordingActuatorBackend::default());

        let mut late_low_pid = sample_plan();
        late_low_pid.target_pid = 11;
        late_low_pid.scenario = ScenarioKind::ToolCallBooster;
        late_low_pid.duration_ms = 700;
        actuator.apply(late_low_pid, 1_000, true);

        let mut early_high_pid = sample_plan();
        early_high_pid.target_pid = 99;
        early_high_pid.scenario = ScenarioKind::InferenceTailGuard;
        early_high_pid.duration_ms = 300;
        actuator.apply(early_high_pid, 1_000, true);

        let mut early_low_pid = sample_plan();
        early_low_pid.target_pid = 7;
        early_low_pid.scenario = ScenarioKind::ToolCallBooster;
        early_low_pid.duration_ms = 300;
        actuator.apply(early_low_pid, 1_000, true);

        let mut early_same_pid_lower_scenario = sample_plan();
        early_same_pid_lower_scenario.target_pid = 7;
        early_same_pid_lower_scenario.scenario = ScenarioKind::InferenceTailGuard;
        early_same_pid_lower_scenario.duration_ms = 300;
        actuator.apply(early_same_pid_lower_scenario, 1_000, true);

        assert!(actuator.expire(1_299).is_empty());

        let rollbacks = actuator.expire(1_700);
        let rollback_order = rollbacks
            .iter()
            .map(|action| {
                (
                    action.expires_at_ms,
                    action.target_pid,
                    action.scenario.clone(),
                )
            })
            .collect::<Vec<_>>();

        assert_eq!(
            rollback_order,
            vec![
                (1_300, 7, ScenarioKind::InferenceTailGuard),
                (1_300, 7, ScenarioKind::ToolCallBooster),
                (1_300, 99, ScenarioKind::InferenceTailGuard),
                (1_700, 11, ScenarioKind::ToolCallBooster),
            ]
        );
        assert_eq!(
            rollbacks
                .iter()
                .map(|rollback| rollback
                    .audit_fields
                    .get("backend.rollback.operation_index")
                    .cloned())
                .collect::<Vec<_>>(),
            vec![
                Some("5".to_string()),
                Some("6".to_string()),
                Some("7".to_string()),
                Some("8".to_string()),
            ]
        );
        assert_eq!(actuator.active_count(), 0);
    }

    #[test]
    fn apply_uses_saturating_expiry_at_timestamp_boundary() {
        let mut actuator = Actuator::default();
        let applied = actuator.apply(sample_plan(), u64::MAX - 10, true);

        assert_eq!(applied.expires_at_ms, u64::MAX);
        assert_eq!(actuator.active_count(), 1);
        assert!(actuator.expire(u64::MAX - 1).is_empty());
        assert_eq!(actuator.expire(u64::MAX).len(), 1);
    }

    #[test]
    fn noop_backend_annotates_apply_and_rollback_audit_fields() {
        let mut actuator = Actuator::with_backend(NoopActuatorBackend);
        let applied = actuator.apply(sample_plan(), 2_000, true);

        assert_eq!(actuator.backend_name(), "noop");
        assert_eq!(
            applied.audit_fields.get("backend.apply.backend"),
            Some(&"noop".to_string())
        );

        let rollbacks = actuator.expire(2_800);
        assert_eq!(rollbacks.len(), 1);
        assert_eq!(
            rollbacks[0].audit_fields.get("backend.rollback.backend"),
            Some(&"noop".to_string())
        );
    }

    #[test]
    fn linux_backend_is_available_as_a_skeleton_backend() {
        let executor = PlannedOnlyLinuxSyscallExecutor::with_state_provider(
            UnavailableLinuxProcessStateProvider,
        );
        let mut actuator = Actuator::with_backend(LinuxActuatorBackend::with_executor(executor));
        let applied = actuator.apply(sample_plan(), 3_000, true);

        assert_eq!(actuator.backend_name(), "linux-skeleton");
        assert_eq!(
            applied.audit_fields.get("backend.apply.mode"),
            Some(&"planned_only".to_string())
        );
        assert_eq!(
            applied.audit_fields.get("backend.apply.syscall.0"),
            Some(&"set_nice:-5".to_string())
        );
        assert_eq!(
            applied
                .audit_fields
                .get("backend.apply.capture.nice.captured"),
            Some(&"false".to_string())
        );
        assert_eq!(
            applied
                .audit_fields
                .get("backend.apply.capture.affinity.captured"),
            Some(&"false".to_string())
        );

        let rollbacks = actuator.expire(3_800);
        assert_eq!(rollbacks.len(), 1);
        assert_eq!(
            rollbacks[0]
                .audit_fields
                .get("backend.rollback.rollback.missing_state"),
            Some(&"nice,affinity".to_string())
        );
        assert_eq!(
            rollbacks[0]
                .audit_fields
                .get("backend.rollback.rollback.0.status"),
            Some(&"missing_state".to_string())
        );
        assert_eq!(
            rollbacks[0]
                .audit_fields
                .get("backend.rollback.rollback.0.detail"),
            Some(&"missing original nice state; rollback skipped".to_string())
        );
        assert_eq!(
            rollbacks[0]
                .audit_fields
                .get("backend.rollback.rollback.1.status"),
            Some(&"missing_state".to_string())
        );
        assert_eq!(
            rollbacks[0]
                .audit_fields
                .get("backend.rollback.rollback.1.detail"),
            Some(&"missing original affinity state; rollback skipped".to_string())
        );
    }

    #[test]
    fn linux_backend_audits_named_backend_on_apply_and_rollback() {
        let executor = PlannedOnlyLinuxSyscallExecutor::with_state_provider_and_applier(
            FakeLinuxProcessStateProvider,
            FakeLinuxSyscallApplier::new(),
        );
        let mut actuator = Actuator::with_backend(LinuxActuatorBackend::with_named_executor(
            "linux-command",
            executor,
        ));
        let applied = actuator.apply(sample_plan(), 3_500, true);

        assert_eq!(actuator.backend_name(), "linux-command");
        assert_eq!(
            applied.audit_fields.get("backend.apply.backend"),
            Some(&"linux-command".to_string())
        );
        assert_eq!(
            applied.audit_fields.get("backend.apply.syscall_executor"),
            Some(&"planned-only".to_string())
        );
        assert_eq!(
            applied.audit_fields.get("backend.apply.timestamp_ms"),
            Some(&"3500".to_string())
        );
        assert_eq!(
            applied.audit_fields.get("backend.apply.target_pid"),
            Some(&"42".to_string())
        );
        assert_eq!(
            applied.audit_fields.get("backend.apply.lease.backend"),
            Some(&"planned-only".to_string())
        );

        let rollbacks = actuator.expire(4_300);
        assert_eq!(rollbacks.len(), 1);
        assert_eq!(
            rollbacks[0].audit_fields.get("backend.rollback.backend"),
            Some(&"linux-command".to_string())
        );
        assert_eq!(
            rollbacks[0]
                .audit_fields
                .get("backend.rollback.syscall_executor"),
            Some(&"planned-only".to_string())
        );
        assert_eq!(
            rollbacks[0].audit_fields.get("backend.rollback.phase"),
            Some(&"rollback".to_string())
        );
        assert_eq!(
            rollbacks[0]
                .audit_fields
                .get("backend.rollback.timestamp_ms"),
            Some(&"4300".to_string())
        );
        assert_eq!(
            rollbacks[0].audit_fields.get("backend.rollback.target_pid"),
            Some(&"42".to_string())
        );
        assert_eq!(
            rollbacks[0]
                .audit_fields
                .get("backend.rollback.lease.linux.applier"),
            Some(&"fake-applier".to_string())
        );
    }

    struct FakeLinuxProcessStateProvider;

    impl LinuxProcessStateProvider for FakeLinuxProcessStateProvider {
        fn provider_name(&self) -> &str {
            "fake-state"
        }

        fn capture_nice(&self, pid: u32) -> LinuxNiceState {
            assert_eq!(pid, 42);
            LinuxNiceState {
                captured: true,
                original_nice: Some(7),
            }
        }

        fn capture_affinity(&self, pid: u32) -> LinuxAffinityState {
            assert_eq!(pid, 42);
            LinuxAffinityState {
                captured: true,
                original_cpus: vec![0, 2, 4],
            }
        }

        fn capture_cpuset(&self, pid: u32) -> LinuxCpusetState {
            assert_eq!(pid, 42);
            LinuxCpusetState {
                captured: true,
                original_cpuset: Some("/sys/fs/cgroup/aegisai/latency".to_string()),
                was_enabled: Some(true),
            }
        }
    }

    struct FakeLinuxSyscallApplier {
        applied: Vec<String>,
        rolled_back: Vec<String>,
    }

    impl FakeLinuxSyscallApplier {
        fn new() -> Self {
            Self {
                applied: Vec::new(),
                rolled_back: Vec::new(),
            }
        }
    }

    struct RollbackFailingLinuxSyscallApplier;

    struct FakeCommandRunner {
        calls: Rc<RefCell<Vec<String>>>,
    }

    impl FakeCommandRunner {
        fn new(calls: Rc<RefCell<Vec<String>>>) -> Self {
            Self { calls }
        }
    }

    impl LinuxCommandRunner for FakeCommandRunner {
        fn runner_name(&self) -> &str {
            "fake-command-runner"
        }

        fn run(&mut self, program: &str, args: &[String]) -> Result<String, String> {
            let line = std::iter::once(program.to_string())
                .chain(args.iter().cloned())
                .collect::<Vec<_>>()
                .join(" ");
            self.calls.borrow_mut().push(line.clone());
            Ok(line)
        }
    }

    struct FakeWarmupExecutorRunner {
        calls: Rc<RefCell<Vec<String>>>,
        result: Result<String, String>,
    }

    impl FakeWarmupExecutorRunner {
        fn new(calls: Rc<RefCell<Vec<String>>>, result: Result<String, String>) -> Self {
            Self { calls, result }
        }
    }

    impl WarmupExecutorRunner for FakeWarmupExecutorRunner {
        fn runner_name(&self) -> &str {
            "fake-warmup-runner"
        }

        fn run_warmup(
            &mut self,
            command: &WarmupExecutorCommand,
            timeout_ms: u64,
        ) -> Result<String, String> {
            self.calls.borrow_mut().push(format!(
                "timeout_ms={timeout_ms};command={}",
                std::iter::once(command.program.clone())
                    .chain(command.args.iter().cloned())
                    .collect::<Vec<_>>()
                    .join(" ")
            ));
            self.result.clone()
        }
    }

    struct DenyingPriorityRaiseCommandRunner {
        calls: Rc<RefCell<Vec<String>>>,
    }

    impl DenyingPriorityRaiseCommandRunner {
        fn new(calls: Rc<RefCell<Vec<String>>>) -> Self {
            Self { calls }
        }
    }

    impl LinuxCommandRunner for DenyingPriorityRaiseCommandRunner {
        fn runner_name(&self) -> &str {
            "denying-priority-raise-runner"
        }

        fn run(&mut self, program: &str, args: &[String]) -> Result<String, String> {
            let line = std::iter::once(program.to_string())
                .chain(args.iter().cloned())
                .collect::<Vec<_>>()
                .join(" ");
            self.calls.borrow_mut().push(line.clone());
            if program == "renice" && args.first().is_some_and(|value| value == "2") {
                Err("permission denied".to_string())
            } else {
                Ok(line)
            }
        }
    }

    struct MissingAffinityLinuxProcessStateProvider;

    impl LinuxProcessStateProvider for MissingAffinityLinuxProcessStateProvider {
        fn provider_name(&self) -> &str {
            "missing-affinity"
        }

        fn capture_nice(&self, _pid: u32) -> LinuxNiceState {
            LinuxNiceState {
                captured: true,
                original_nice: Some(5),
            }
        }

        fn capture_affinity(&self, _pid: u32) -> LinuxAffinityState {
            LinuxAffinityState::default()
        }

        fn capture_cpuset(&self, _pid: u32) -> LinuxCpusetState {
            LinuxCpusetState::default()
        }
    }

    impl LinuxSyscallApplier for FakeLinuxSyscallApplier {
        fn applier_name(&self) -> &str {
            "fake-applier"
        }

        fn apply_operation(
            &mut self,
            target_pid: u32,
            operation: &LinuxSyscallOperation,
            _captured_state: &crate::LinuxCapturedState,
            _now_ms: u64,
        ) -> Result<String, String> {
            self.applied.push(format!("{target_pid}:{operation:?}"));
            Ok("applied".to_string())
        }

        fn rollback_operation(
            &mut self,
            target_pid: u32,
            operation: &LinuxSyscallOperation,
            _captured_state: &crate::LinuxCapturedState,
            _now_ms: u64,
        ) -> Result<String, String> {
            self.rolled_back.push(format!("{target_pid}:{operation:?}"));
            Ok("rolled_back".to_string())
        }
    }

    impl LinuxSyscallApplier for RollbackFailingLinuxSyscallApplier {
        fn applier_name(&self) -> &str {
            "rollback-failing-applier"
        }

        fn apply_operation(
            &mut self,
            _target_pid: u32,
            _operation: &LinuxSyscallOperation,
            _captured_state: &crate::LinuxCapturedState,
            _now_ms: u64,
        ) -> Result<String, String> {
            Ok("applied".to_string())
        }

        fn rollback_operation(
            &mut self,
            _target_pid: u32,
            operation: &LinuxSyscallOperation,
            _captured_state: &crate::LinuxCapturedState,
            _now_ms: u64,
        ) -> Result<String, String> {
            match operation {
                LinuxSyscallOperation::RestoreAffinity => {
                    Err("rollback affinity denied".to_string())
                }
                _ => Ok("rolled_back".to_string()),
            }
        }
    }

    #[test]
    fn planned_executor_can_capture_original_linux_state_from_provider() {
        let executor = PlannedOnlyLinuxSyscallExecutor::with_state_provider_and_applier(
            FakeLinuxProcessStateProvider,
            FakeLinuxSyscallApplier::new(),
        );
        let mut actuator = Actuator::with_backend(LinuxActuatorBackend::with_executor(executor));
        let applied = actuator.apply(sample_plan(), 4_000, true);

        assert_eq!(
            applied.audit_fields.get("backend.apply.capture.provider"),
            Some(&"fake-state".to_string())
        );
        assert_eq!(
            applied
                .audit_fields
                .get("backend.apply.capture.nice.captured"),
            Some(&"true".to_string())
        );
        assert_eq!(
            applied
                .audit_fields
                .get("backend.apply.capture.affinity.captured"),
            Some(&"true".to_string())
        );
        assert_eq!(
            applied.audit_fields.get("backend.apply.applier"),
            Some(&"fake-applier".to_string())
        );
        assert_eq!(
            applied.audit_fields.get("backend.apply.apply.0.status"),
            Some(&"ok".to_string())
        );

        let rollbacks = actuator.expire(4_800);
        assert_eq!(rollbacks.len(), 1);
        assert_eq!(
            rollbacks[0]
                .audit_fields
                .get("backend.rollback.rollback.restored"),
            Some(&"nice,affinity".to_string())
        );
        assert_eq!(
            rollbacks[0]
                .audit_fields
                .get("backend.rollback.rollback.0.status"),
            Some(&"ok".to_string())
        );
        assert_eq!(
            rollbacks[0]
                .audit_fields
                .get("backend.rollback.rollback.missing_state"),
            None
        );
    }

    #[test]
    fn linux_apply_success_reports_rollback_failure_trace() {
        let executor = PlannedOnlyLinuxSyscallExecutor::with_state_provider_and_applier(
            FakeLinuxProcessStateProvider,
            RollbackFailingLinuxSyscallApplier,
        );
        let mut actuator = Actuator::with_backend(LinuxActuatorBackend::with_executor(executor));

        let applied = actuator.apply(sample_plan(), 4_900, true);

        assert_eq!(
            applied.audit_fields.get("backend.apply.apply.result"),
            Some(&"ok".to_string())
        );
        assert_eq!(
            applied.audit_fields.get("backend.apply.apply.failed_count"),
            Some(&"0".to_string())
        );

        let rollbacks = actuator.expire(5_700);
        assert_eq!(rollbacks.len(), 1);
        assert_eq!(
            rollbacks[0]
                .audit_fields
                .get("backend.rollback.rollback.restored"),
            Some(&"nice".to_string())
        );
        assert_eq!(
            rollbacks[0]
                .audit_fields
                .get("backend.rollback.rollback.failed"),
            Some(&"affinity:rollback affinity denied".to_string())
        );
        assert_eq!(
            rollbacks[0]
                .audit_fields
                .get("backend.rollback.rollback.0.status"),
            Some(&"ok".to_string())
        );
        assert_eq!(
            rollbacks[0]
                .audit_fields
                .get("backend.rollback.rollback.1.status"),
            Some(&"error".to_string())
        );
        assert_eq!(
            rollbacks[0]
                .audit_fields
                .get("backend.rollback.rollback.1.error"),
            Some(&"rollback affinity denied".to_string())
        );
    }

    #[test]
    fn command_applier_executes_apply_and_rollback_commands() {
        let calls = Rc::new(RefCell::new(Vec::new()));
        let applier =
            CommandLinuxSyscallApplier::with_runner(FakeCommandRunner::new(calls.clone()));
        let executor = PlannedOnlyLinuxSyscallExecutor::with_state_provider_and_applier(
            FakeLinuxProcessStateProvider,
            applier,
        );
        let mut actuator = Actuator::with_backend(LinuxActuatorBackend::with_executor(executor));

        let applied = actuator.apply(sample_plan(), 5_000, true);
        assert_eq!(
            applied.audit_fields.get("backend.apply.applier"),
            Some(&"command".to_string())
        );

        let rollbacks = actuator.expire(5_800);
        assert_eq!(rollbacks.len(), 1);

        let commands = calls.borrow();
        assert_eq!(commands.len(), 4);
        assert_eq!(commands[0], "renice 2 -p 42");
        assert_eq!(commands[1], "taskset -pc 0,2 42");
        assert_eq!(commands[2], "renice 7 -p 42");
        assert_eq!(commands[3], "taskset -pc 0,2,4 42");
    }

    #[test]
    fn command_applier_refresh_reuses_original_affinity_capture() {
        let calls = Rc::new(RefCell::new(Vec::new()));
        let applier =
            CommandLinuxSyscallApplier::with_runner(FakeCommandRunner::new(calls.clone()));
        let executor = PlannedOnlyLinuxSyscallExecutor::with_state_provider_and_applier(
            FakeLinuxProcessStateProvider,
            applier,
        );
        let mut actuator = Actuator::with_backend(LinuxActuatorBackend::with_executor(executor));

        actuator.apply(sample_plan(), 5_000, true);
        actuator.apply(sample_plan(), 5_100, true);
        let rollbacks = actuator.expire(5_900);
        assert_eq!(rollbacks.len(), 1);

        let commands = calls.borrow();
        assert_eq!(commands.len(), 6);
        assert_eq!(commands[1], "taskset -pc 0,2 42");
        assert_eq!(commands[3], "taskset -pc 0,2 42");
        assert_eq!(commands[5], "taskset -pc 0,2,4 42");
    }

    #[test]
    fn command_applier_audits_dry_run_command_details() {
        let executor = PlannedOnlyLinuxSyscallExecutor::with_state_provider_and_applier(
            FakeLinuxProcessStateProvider,
            CommandLinuxSyscallApplier::dry_run(),
        );
        let mut actuator = Actuator::with_backend(LinuxActuatorBackend::with_executor(executor));

        let applied = actuator.apply(sample_plan(), 6_000, true);

        assert_eq!(
            applied.audit_fields.get("backend.apply.apply.result"),
            Some(&"ok".to_string())
        );
        assert_eq!(
            applied.audit_fields.get("backend.apply.apply.0.detail"),
            Some(
                &"runner=dry-run-command-runner;command=renice 2 -p 42;output=dry_run:renice 2 -p 42"
                    .to_string()
            )
        );
        assert_eq!(
            applied.audit_fields.get("backend.apply.apply.1.detail"),
            Some(
                &"runner=dry-run-command-runner;command=taskset -pc 0,2 42;output=dry_run:taskset -pc 0,2 42"
                    .to_string()
            )
        );
    }

    #[test]
    fn disabled_cpuset_action_does_not_emit_cpuset_rollback_noise() {
        let executor = PlannedOnlyLinuxSyscallExecutor::with_state_provider_and_applier(
            FakeLinuxProcessStateProvider,
            CommandLinuxSyscallApplier::dry_run(),
        );
        let mut actuator = Actuator::with_backend(LinuxActuatorBackend::with_named_executor(
            "linux-command-dry-run",
            executor,
        ));

        let applied = actuator.apply(sample_plan_with_disabled_cpuset(), 6_500, true);

        assert_eq!(
            applied.audit_fields.get("backend.apply.apply.2.detail"),
            Some(&"cpuset disabled by policy".to_string())
        );
        assert_eq!(
            applied
                .audit_fields
                .get("backend.apply.capture.cpuset.captured"),
            None
        );

        let rollbacks = actuator.expire(7_300);
        assert_eq!(rollbacks.len(), 1);
        assert_eq!(
            rollbacks[0]
                .audit_fields
                .get("backend.rollback.rollback.restored"),
            Some(&"nice,affinity".to_string())
        );
        assert_eq!(
            rollbacks[0]
                .audit_fields
                .get("backend.rollback.rollback.failed"),
            None
        );
        assert!(!rollbacks[0]
            .audit_fields
            .values()
            .any(|value| value.contains("cpuset restore requires")));
    }

    #[test]
    fn warmup_executor_runs_configured_side_effect_and_rolls_back_as_noop() {
        let warmup_calls = Rc::new(RefCell::new(Vec::new()));
        let applier = CommandLinuxSyscallApplier::dry_run().with_warmup_executor_runner(
            FakeWarmupExecutorRunner::new(
                warmup_calls.clone(),
                Ok("warmup executor applied;side_effect=command;elapsed_ms=7".to_string()),
            ),
            WarmupExecutorCommand::new(
                "python3",
                [
                    "bench/tool_call_booster/real_tool_executor.py".to_string(),
                    "retrieval-worker".to_string(),
                ],
            ),
            125,
        );
        let executor = PlannedOnlyLinuxSyscallExecutor::with_state_provider_and_applier(
            FakeLinuxProcessStateProvider,
            applier,
        );
        let mut actuator = Actuator::with_backend(LinuxActuatorBackend::with_executor(executor));
        let mut plan = sample_plan();
        plan.actions = vec![Action::WarmupExecutor];

        let applied = actuator.apply(plan, 6_575, true);

        assert_eq!(
            applied.audit_fields.get("backend.apply.apply.result"),
            Some(&"ok".to_string())
        );
        assert_eq!(
            applied.audit_fields.get("backend.apply.apply.0.detail"),
            Some(
                &"runner=fake-warmup-runner;warmup executor applied;side_effect=command;elapsed_ms=7"
                    .to_string()
            )
        );
        assert_eq!(
            warmup_calls.borrow().as_slice(),
            ["timeout_ms=125;command=python3 bench/tool_call_booster/real_tool_executor.py retrieval-worker"]
        );

        let rollbacks = actuator.expire(7_375);
        assert_eq!(rollbacks.len(), 1);
        assert_eq!(
            rollbacks[0]
                .audit_fields
                .get("backend.rollback.rollback.skipped"),
            Some(&"warmup_executor".to_string())
        );
        assert_eq!(
            rollbacks[0]
                .audit_fields
                .get("backend.rollback.rollback.0.status"),
            Some(&"ok".to_string())
        );
        assert_eq!(
            rollbacks[0]
                .audit_fields
                .get("backend.rollback.rollback.0.detail"),
            Some(&"warmup rollback noop".to_string())
        );
        assert_eq!(
            rollbacks[0]
                .audit_fields
                .get("backend.rollback.rollback.restored"),
            None
        );
    }

    #[test]
    fn warmup_executor_timeout_is_audited_as_apply_failure() {
        let warmup_calls = Rc::new(RefCell::new(Vec::new()));
        let applier = CommandLinuxSyscallApplier::dry_run().with_warmup_executor_runner(
            FakeWarmupExecutorRunner::new(
                warmup_calls.clone(),
                Err("warmup executor command `prime-cache` timed out after 10ms".to_string()),
            ),
            WarmupExecutorCommand::new("prime-cache", []),
            10,
        );
        let executor = PlannedOnlyLinuxSyscallExecutor::with_state_provider_and_applier(
            FakeLinuxProcessStateProvider,
            applier,
        );
        let mut actuator = Actuator::with_backend(LinuxActuatorBackend::with_executor(executor));
        let mut plan = sample_plan();
        plan.actions = vec![Action::WarmupExecutor];

        let applied = actuator.apply(plan, 6_590, true);

        assert_eq!(
            applied.audit_fields.get("backend.apply.apply.result"),
            Some(&"error".to_string())
        );
        assert_eq!(
            applied.audit_fields.get("backend.apply.apply.failed_count"),
            Some(&"1".to_string())
        );
        assert!(applied
            .audit_fields
            .get("backend.apply.apply.0.error")
            .is_some_and(|value| value
                .contains("warmup executor command `prime-cache` timed out after 10ms")));
        assert_eq!(
            warmup_calls.borrow().as_slice(),
            ["timeout_ms=10;command=prime-cache"]
        );
    }

    #[test]
    fn live_command_guard_stage_one_applies_only_nice_and_rolls_back_only_nice() {
        let calls = Rc::new(RefCell::new(Vec::new()));
        let guard = LiveLinuxCommandGuard::nice_only([42], true);
        let applier = CommandLinuxSyscallApplier::guarded_live(
            FakeCommandRunner::new(calls.clone()),
            guard.clone(),
        );
        let executor = PlannedOnlyLinuxSyscallExecutor::with_state_provider_and_applier(
            FakeLinuxProcessStateProvider,
            applier,
        )
        .with_live_guard(guard);
        let mut actuator = Actuator::with_backend(LinuxActuatorBackend::with_named_executor(
            "linux-command",
            executor,
        ));

        let applied = actuator.apply(sample_plan_with_disabled_cpuset(), 6_600, true);

        assert_eq!(
            applied.audit_fields.get("backend.apply.live_guard.scope"),
            Some(&"nice".to_string())
        );
        assert_eq!(
            applied
                .audit_fields
                .get("backend.apply.live_guard.target_allowed"),
            Some(&"true".to_string())
        );
        assert_eq!(
            applied.audit_fields.get("backend.apply.apply.0.status"),
            Some(&"ok".to_string())
        );
        assert_eq!(
            applied.audit_fields.get("backend.apply.apply.1.status"),
            Some(&"skipped".to_string())
        );
        assert_eq!(
            applied.audit_fields.get("backend.apply.apply.2.status"),
            Some(&"ok".to_string())
        );
        assert_eq!(
            applied
                .audit_fields
                .get("backend.apply.apply.skipped_count"),
            Some(&"1".to_string())
        );
        assert_eq!(
            applied
                .audit_fields
                .get("backend.apply.lease.linux.live_guard.scope"),
            Some(&"nice".to_string())
        );
        assert_eq!(
            applied
                .audit_fields
                .get("backend.apply.lease.linux.nice.original"),
            Some(&"7".to_string())
        );

        let rollbacks = actuator.expire(7_400);
        assert_eq!(rollbacks.len(), 1);
        assert_eq!(
            rollbacks[0]
                .audit_fields
                .get("backend.rollback.rollback.restored"),
            Some(&"nice".to_string())
        );
        assert_eq!(
            rollbacks[0]
                .audit_fields
                .get("backend.rollback.lease.linux.nice.original"),
            Some(&"7".to_string())
        );
        assert_eq!(
            rollbacks[0]
                .audit_fields
                .get("backend.rollback.rollback.skipped"),
            Some(&"affinity".to_string())
        );
        assert_eq!(
            rollbacks[0]
                .audit_fields
                .get("backend.rollback.rollback.failed"),
            None
        );

        let commands = calls.borrow();
        assert_eq!(commands.as_slice(), ["renice 2 -p 42", "renice 7 -p 42"]);
    }

    #[test]
    fn live_command_guard_can_degrade_priority_raise_to_noop_nice() {
        let calls = Rc::new(RefCell::new(Vec::new()));
        let guard = LiveLinuxCommandGuard::nice_only([42], true).without_priority_raise();
        let applier = CommandLinuxSyscallApplier::guarded_live(
            DenyingPriorityRaiseCommandRunner::new(calls.clone()),
            guard.clone(),
        );
        let executor = PlannedOnlyLinuxSyscallExecutor::with_state_provider_and_applier(
            FakeLinuxProcessStateProvider,
            applier,
        )
        .with_live_guard(guard);
        let mut actuator = Actuator::with_backend(LinuxActuatorBackend::with_named_executor(
            "linux-command",
            executor,
        ));

        let applied = actuator.apply(sample_plan_with_disabled_cpuset(), 6_625, true);

        assert_eq!(
            applied
                .audit_fields
                .get("backend.apply.live_guard.priority_raise_allowed"),
            Some(&"false".to_string())
        );
        assert_eq!(
            applied.audit_fields.get("backend.apply.apply.result"),
            Some(&"ok".to_string())
        );
        assert_eq!(
            applied.audit_fields.get("backend.apply.apply.0.status"),
            Some(&"ok".to_string())
        );
        assert!(applied
            .audit_fields
            .get("backend.apply.apply.0.detail")
            .is_some_and(|value| value.contains("priority_raise_limited=true")));

        let rollbacks = actuator.expire(7_425);
        assert_eq!(rollbacks.len(), 1);
        assert_eq!(
            rollbacks[0]
                .audit_fields
                .get("backend.rollback.rollback.failed"),
            None
        );

        let commands = calls.borrow();
        assert_eq!(commands.as_slice(), ["renice 7 -p 42", "renice 7 -p 42"]);
    }

    #[test]
    fn live_command_guard_keeps_cpuset_disabled_even_when_policy_requests_it() {
        let calls = Rc::new(RefCell::new(Vec::new()));
        let guard = LiveLinuxCommandGuard::nice_and_affinity([42], true);
        let applier = CommandLinuxSyscallApplier::guarded_live(
            FakeCommandRunner::new(calls.clone()),
            guard.clone(),
        );
        let executor = PlannedOnlyLinuxSyscallExecutor::with_state_provider_and_applier(
            FakeLinuxProcessStateProvider,
            applier,
        )
        .with_live_guard(guard);
        let mut actuator = Actuator::with_backend(LinuxActuatorBackend::with_named_executor(
            "linux-command",
            executor,
        ));
        let mut plan = sample_plan();
        plan.actions.push(Action::UseCpuset { enabled: true });

        let applied = actuator.apply(plan, 6_650, true);

        assert_eq!(
            applied.audit_fields.get("backend.apply.live_guard.scope"),
            Some(&"nice,affinity".to_string())
        );
        assert_eq!(
            applied.audit_fields.get("backend.apply.apply.2.status"),
            Some(&"skipped".to_string())
        );
        assert_eq!(
            applied.audit_fields.get("backend.apply.apply.2.detail"),
            Some(&"cpuset command disabled by live guard".to_string())
        );

        let rollbacks = actuator.expire(7_450);
        assert_eq!(rollbacks.len(), 1);
        assert_eq!(
            rollbacks[0]
                .audit_fields
                .get("backend.rollback.rollback.restored"),
            Some(&"nice,affinity".to_string())
        );
        assert_eq!(
            rollbacks[0]
                .audit_fields
                .get("backend.rollback.rollback.skipped"),
            Some(&"cpuset".to_string())
        );
        assert!(!rollbacks[0]
            .audit_fields
            .values()
            .any(|value| value.contains("cpuset restore requires")));

        let commands = calls.borrow();
        assert_eq!(commands.len(), 4);
        assert!(!commands.iter().any(|command| command.contains("cpuset")));
    }

    #[test]
    fn live_command_guard_stage_two_applies_nice_and_affinity_with_rollback() {
        let calls = Rc::new(RefCell::new(Vec::new()));
        let guard = LiveLinuxCommandGuard::nice_and_affinity([42], true);
        let applier = CommandLinuxSyscallApplier::guarded_live(
            FakeCommandRunner::new(calls.clone()),
            guard.clone(),
        );
        let executor = PlannedOnlyLinuxSyscallExecutor::with_state_provider_and_applier(
            FakeLinuxProcessStateProvider,
            applier,
        )
        .with_live_guard(guard);
        let mut actuator = Actuator::with_backend(LinuxActuatorBackend::with_named_executor(
            "linux-command",
            executor,
        ));

        let applied = actuator.apply(sample_plan(), 6_700, true);
        assert_eq!(
            applied.audit_fields.get("backend.apply.live_guard.scope"),
            Some(&"nice,affinity".to_string())
        );
        assert_eq!(
            applied
                .audit_fields
                .get("backend.apply.apply.applied_count"),
            Some(&"2".to_string())
        );
        assert_eq!(
            applied
                .audit_fields
                .get("backend.apply.apply.skipped_count"),
            Some(&"0".to_string())
        );

        let rollbacks = actuator.expire(7_500);
        assert_eq!(rollbacks.len(), 1);
        assert_eq!(
            rollbacks[0]
                .audit_fields
                .get("backend.rollback.rollback.restored"),
            Some(&"nice,affinity".to_string())
        );

        let commands = calls.borrow();
        assert_eq!(commands.len(), 4);
        assert_eq!(commands[0], "renice 2 -p 42");
        assert_eq!(commands[1], "taskset -pc 0,2 42");
        assert_eq!(commands[2], "renice 7 -p 42");
        assert_eq!(commands[3], "taskset -pc 0,2,4 42");
    }

    #[test]
    fn live_command_guard_rejects_pid_outside_allowlist_before_commands() {
        let calls = Rc::new(RefCell::new(Vec::new()));
        let guard = LiveLinuxCommandGuard::nice_and_affinity([77], true);
        let applier = CommandLinuxSyscallApplier::guarded_live(
            FakeCommandRunner::new(calls.clone()),
            guard.clone(),
        );
        let executor = PlannedOnlyLinuxSyscallExecutor::with_state_provider_and_applier(
            FakeLinuxProcessStateProvider,
            applier,
        )
        .with_live_guard(guard);
        let mut actuator = Actuator::with_backend(LinuxActuatorBackend::with_named_executor(
            "linux-command",
            executor,
        ));

        let applied = actuator.apply(sample_plan(), 6_800, true);

        assert_eq!(
            applied
                .audit_fields
                .get("backend.apply.live_guard.target_allowed"),
            Some(&"false".to_string())
        );
        assert_eq!(
            applied.audit_fields.get("backend.apply.apply.result"),
            Some(&"error".to_string())
        );
        assert_eq!(
            applied.audit_fields.get("backend.apply.apply.failed_count"),
            Some(&"2".to_string())
        );
        assert!(applied
            .audit_fields
            .get("backend.apply.apply.0.error")
            .is_some_and(|value| value.contains("PID allowlist")));
        assert!(calls.borrow().is_empty());
    }

    #[test]
    fn linux_apply_reports_partial_command_application() {
        let calls = Rc::new(RefCell::new(Vec::new()));
        let applier =
            CommandLinuxSyscallApplier::with_runner(FakeCommandRunner::new(calls.clone()));
        let executor = PlannedOnlyLinuxSyscallExecutor::with_state_provider_and_applier(
            MissingAffinityLinuxProcessStateProvider,
            applier,
        );
        let mut actuator = Actuator::with_backend(LinuxActuatorBackend::with_executor(executor));

        let applied = actuator.apply(sample_plan(), 7_000, true);

        assert_eq!(
            applied.audit_fields.get("backend.apply.apply.result"),
            Some(&"partial".to_string())
        );
        assert_eq!(
            applied.audit_fields.get("backend.apply.apply.partial"),
            Some(&"true".to_string())
        );
        assert_eq!(
            applied
                .audit_fields
                .get("backend.apply.apply.applied_count"),
            Some(&"1".to_string())
        );
        assert_eq!(
            applied.audit_fields.get("backend.apply.apply.failed_count"),
            Some(&"1".to_string())
        );
        assert_eq!(
            applied.audit_fields.get("backend.apply.apply.1.error"),
            Some(&"original affinity state was not captured".to_string())
        );

        let commands = calls.borrow();
        assert_eq!(commands.as_slice(), ["renice 0 -p 42"]);
    }

    #[test]
    fn command_applier_refuses_pid_zero_before_running_commands() {
        let calls = Rc::new(RefCell::new(Vec::new()));
        let applier =
            CommandLinuxSyscallApplier::with_runner(FakeCommandRunner::new(calls.clone()));
        let executor = PlannedOnlyLinuxSyscallExecutor::with_state_provider_and_applier(
            MissingAffinityLinuxProcessStateProvider,
            applier,
        );
        let mut actuator = Actuator::with_backend(LinuxActuatorBackend::with_executor(executor));
        let mut plan = sample_plan();
        plan.target_pid = 0;
        plan.actions = vec![Action::RaiseNice { delta: -5 }];

        let applied = actuator.apply(plan, 8_000, true);

        assert_eq!(
            applied.audit_fields.get("backend.apply.apply.result"),
            Some(&"error".to_string())
        );
        assert_eq!(
            applied.audit_fields.get("backend.apply.apply.0.error"),
            Some(&"refusing to apply Linux command to pid 0".to_string())
        );
        assert!(calls.borrow().is_empty());
    }

    #[test]
    fn default_command_applier_requires_guarded_live_constructor() {
        let executor = PlannedOnlyLinuxSyscallExecutor::with_state_provider_and_applier(
            FakeLinuxProcessStateProvider,
            CommandLinuxSyscallApplier::new(),
        );
        let mut actuator = Actuator::with_backend(LinuxActuatorBackend::with_named_executor(
            "linux-command",
            executor,
        ));
        let mut plan = sample_plan();
        plan.actions = vec![Action::RaiseNice { delta: -5 }];

        let applied = actuator.apply(plan, 8_100, true);

        assert_eq!(
            applied.audit_fields.get("backend.apply.apply.result"),
            Some(&"error".to_string())
        );
        assert!(applied
            .audit_fields
            .get("backend.apply.apply.0.error")
            .is_some_and(|value| value.contains("explicit confirmation")));
    }
}
