use std::collections::{BTreeMap, BTreeSet};

use crate::cpu_affinity::{parse_status_cpu_list, CpuAffinityPlanner};
use crate::model::{Action, ActionPlan, AppliedAction, ScenarioKind};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BackendExecution {
    pub audit_fields: BTreeMap<String, String>,
}

impl BackendExecution {
    pub fn with_field(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.audit_fields.insert(key.into(), value.into());
        self
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BackendLease {
    pub backend_name: String,
    pub captured_state: BTreeMap<String, String>,
}

impl BackendLease {
    pub fn new(backend_name: impl Into<String>) -> Self {
        Self {
            backend_name: backend_name.into(),
            captured_state: BTreeMap::new(),
        }
    }

    pub fn with_field(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.captured_state.insert(key.into(), value.into());
        self
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BackendApplyResult {
    pub execution: BackendExecution,
    pub lease: Option<BackendLease>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinuxSyscallPhase {
    Apply,
    Rollback,
}

#[derive(Clone, Debug, PartialEq)]
pub enum LinuxSyscallOperation {
    SetNice {
        delta: i32,
    },
    SetAffinity {
        strategy: String,
        max_cpu_ratio: f32,
    },
    UseCpuset {
        enabled: bool,
    },
    WarmupExecutor,
    RestoreNice,
    RestoreAffinity,
    RestoreCpuset,
    NoopWarmupRollback,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LinuxSyscallPlan {
    pub phase: LinuxSyscallPhase,
    pub target_pid: u32,
    pub operations: Vec<LinuxSyscallOperation>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LinuxNiceState {
    pub captured: bool,
    pub original_nice: Option<i32>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LinuxAffinityState {
    pub captured: bool,
    pub original_cpus: Vec<u32>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LinuxCpusetState {
    pub captured: bool,
    pub original_cpuset: Option<String>,
    pub was_enabled: Option<bool>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LinuxCapturedState {
    pub nice: Option<LinuxNiceState>,
    pub affinity: Option<LinuxAffinityState>,
    pub cpuset: Option<LinuxCpusetState>,
}

impl LinuxCapturedState {
    pub fn placeholder_for_plan(plan: &LinuxSyscallPlan) -> Self {
        let mut state = Self::default();

        for operation in &plan.operations {
            match operation {
                LinuxSyscallOperation::SetNice { .. } => {
                    state.nice.get_or_insert_with(LinuxNiceState::default);
                }
                LinuxSyscallOperation::SetAffinity { .. } => {
                    state
                        .affinity
                        .get_or_insert_with(LinuxAffinityState::default);
                }
                LinuxSyscallOperation::UseCpuset { .. } => {
                    state.cpuset.get_or_insert_with(LinuxCpusetState::default);
                }
                LinuxSyscallOperation::WarmupExecutor
                | LinuxSyscallOperation::RestoreNice
                | LinuxSyscallOperation::RestoreAffinity
                | LinuxSyscallOperation::RestoreCpuset
                | LinuxSyscallOperation::NoopWarmupRollback => {}
            }
        }

        state
    }

    pub fn apply_to_lease(&self, mut lease: BackendLease) -> BackendLease {
        if let Some(nice) = &self.nice {
            lease = lease
                .with_field("linux.nice.captured", nice.captured.to_string())
                .with_field(
                    "linux.nice.original",
                    nice.original_nice
                        .map(|value| value.to_string())
                        .unwrap_or_default(),
                );
        }

        if let Some(affinity) = &self.affinity {
            lease = lease
                .with_field("linux.affinity.captured", affinity.captured.to_string())
                .with_field(
                    "linux.affinity.original_cpus",
                    affinity
                        .original_cpus
                        .iter()
                        .map(u32::to_string)
                        .collect::<Vec<_>>()
                        .join(","),
                );
        }

        if let Some(cpuset) = &self.cpuset {
            lease = lease
                .with_field("linux.cpuset.captured", cpuset.captured.to_string())
                .with_field(
                    "linux.cpuset.original",
                    cpuset.original_cpuset.clone().unwrap_or_default(),
                )
                .with_field(
                    "linux.cpuset.was_enabled",
                    cpuset
                        .was_enabled
                        .map(|value| value.to_string())
                        .unwrap_or_default(),
                );
        }

        lease
    }

    pub fn from_lease(lease: Option<&BackendLease>) -> Self {
        let Some(lease) = lease else {
            return Self::default();
        };

        Self {
            nice: lease
                .captured_state
                .contains_key("linux.nice.captured")
                .then(|| LinuxNiceState {
                    captured: lease
                        .captured_state
                        .get("linux.nice.captured")
                        .and_then(|value| value.parse::<bool>().ok())
                        .unwrap_or(false),
                    original_nice: lease
                        .captured_state
                        .get("linux.nice.original")
                        .filter(|value| !value.is_empty())
                        .and_then(|value| value.parse::<i32>().ok()),
                }),
            affinity: lease
                .captured_state
                .contains_key("linux.affinity.captured")
                .then(|| LinuxAffinityState {
                    captured: lease
                        .captured_state
                        .get("linux.affinity.captured")
                        .and_then(|value| value.parse::<bool>().ok())
                        .unwrap_or(false),
                    original_cpus: lease
                        .captured_state
                        .get("linux.affinity.original_cpus")
                        .map(String::as_str)
                        .unwrap_or("")
                        .split(',')
                        .filter(|value| !value.is_empty())
                        .filter_map(|value| value.parse::<u32>().ok())
                        .collect(),
                }),
            cpuset: lease
                .captured_state
                .contains_key("linux.cpuset.captured")
                .then(|| LinuxCpusetState {
                    captured: lease
                        .captured_state
                        .get("linux.cpuset.captured")
                        .and_then(|value| value.parse::<bool>().ok())
                        .unwrap_or(false),
                    original_cpuset: lease
                        .captured_state
                        .get("linux.cpuset.original")
                        .filter(|value| !value.is_empty())
                        .cloned(),
                    was_enabled: lease
                        .captured_state
                        .get("linux.cpuset.was_enabled")
                        .filter(|value| !value.is_empty())
                        .and_then(|value| value.parse::<bool>().ok()),
                }),
        }
    }
}

pub trait LinuxProcessStateProvider {
    fn provider_name(&self) -> &str;

    fn capture_nice(&self, pid: u32) -> LinuxNiceState;

    fn capture_affinity(&self, pid: u32) -> LinuxAffinityState;

    fn capture_cpuset(&self, pid: u32) -> LinuxCpusetState;
}

pub trait LinuxSyscallApplier {
    fn applier_name(&self) -> &str;

    fn apply_operation(
        &mut self,
        target_pid: u32,
        operation: &LinuxSyscallOperation,
        captured_state: &LinuxCapturedState,
        now_ms: u64,
    ) -> Result<String, String>;

    fn rollback_operation(
        &mut self,
        target_pid: u32,
        operation: &LinuxSyscallOperation,
        captured_state: &LinuxCapturedState,
        now_ms: u64,
    ) -> Result<String, String>;
}

pub trait LinuxCommandRunner {
    fn runner_name(&self) -> &str;

    fn run(&mut self, program: &str, args: &[String]) -> Result<String, String>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LiveLinuxCommandGuard {
    allowed_pids: BTreeSet<u32>,
    explicit_confirmation: bool,
    enable_nice: bool,
    enable_affinity: bool,
    enable_cpuset: bool,
    allow_priority_raise: bool,
}

impl LiveLinuxCommandGuard {
    pub fn nice_only<I>(allowed_pids: I, explicit_confirmation: bool) -> Self
    where
        I: IntoIterator<Item = u32>,
    {
        Self {
            allowed_pids: allowed_pids.into_iter().collect(),
            explicit_confirmation,
            enable_nice: true,
            enable_affinity: false,
            enable_cpuset: false,
            allow_priority_raise: true,
        }
    }

    pub fn nice_and_affinity<I>(allowed_pids: I, explicit_confirmation: bool) -> Self
    where
        I: IntoIterator<Item = u32>,
    {
        Self {
            enable_affinity: true,
            ..Self::nice_only(allowed_pids, explicit_confirmation)
        }
    }

    pub fn allowed_pids(&self) -> &BTreeSet<u32> {
        &self.allowed_pids
    }

    pub fn without_priority_raise(mut self) -> Self {
        self.allow_priority_raise = false;
        self
    }

    pub fn allows_priority_raise(&self) -> bool {
        self.allow_priority_raise
    }

    fn validate_target(&self, target_pid: u32) -> Result<(), String> {
        if !self.explicit_confirmation {
            return Err("linux-command live actuator requires explicit confirmation".to_string());
        }
        if self.allowed_pids.is_empty() {
            return Err(
                "linux-command live actuator requires a non-empty PID allowlist".to_string(),
            );
        }
        if !self.allowed_pids.contains(&target_pid) {
            return Err(format!(
                "target pid {target_pid} is not in linux-command live actuator PID allowlist"
            ));
        }

        Ok(())
    }

    fn target_allowed(&self, target_pid: u32) -> bool {
        self.explicit_confirmation && self.allowed_pids.contains(&target_pid)
    }

    fn allows_operation(&self, operation: &LinuxSyscallOperation) -> bool {
        match operation {
            LinuxSyscallOperation::SetNice { .. } | LinuxSyscallOperation::RestoreNice => {
                self.enable_nice
            }
            LinuxSyscallOperation::SetAffinity { .. } | LinuxSyscallOperation::RestoreAffinity => {
                self.enable_affinity
            }
            LinuxSyscallOperation::UseCpuset { enabled } => !*enabled || self.enable_cpuset,
            LinuxSyscallOperation::RestoreCpuset => self.enable_cpuset,
            LinuxSyscallOperation::WarmupExecutor | LinuxSyscallOperation::NoopWarmupRollback => {
                true
            }
        }
    }

    fn skipped_detail(&self, operation: &LinuxSyscallOperation) -> String {
        match operation {
            LinuxSyscallOperation::SetNice { .. } | LinuxSyscallOperation::RestoreNice => {
                "nice command disabled by live guard".to_string()
            }
            LinuxSyscallOperation::SetAffinity { .. } | LinuxSyscallOperation::RestoreAffinity => {
                "affinity command disabled by live guard".to_string()
            }
            LinuxSyscallOperation::UseCpuset { enabled: true }
            | LinuxSyscallOperation::RestoreCpuset => {
                "cpuset command disabled by live guard".to_string()
            }
            LinuxSyscallOperation::UseCpuset { enabled: false } => {
                "cpuset disabled by policy".to_string()
            }
            LinuxSyscallOperation::WarmupExecutor => "warmup executor deferred".to_string(),
            LinuxSyscallOperation::NoopWarmupRollback => "warmup rollback noop".to_string(),
        }
    }

    fn bounded_live_nice_target(&self, original_nice: i32, requested_nice: i32) -> (i32, bool) {
        if self.allow_priority_raise || requested_nice >= original_nice {
            (requested_nice, false)
        } else {
            (original_nice, true)
        }
    }

    fn scope_label(&self) -> String {
        let mut enabled = Vec::new();
        if self.enable_nice {
            enabled.push("nice");
        }
        if self.enable_affinity {
            enabled.push("affinity");
        }
        if self.enable_cpuset {
            enabled.push("cpuset");
        }
        if enabled.is_empty() {
            "none".to_string()
        } else {
            enabled.join(",")
        }
    }

    fn allowed_pids_label(&self) -> String {
        self.allowed_pids
            .iter()
            .map(u32::to_string)
            .collect::<Vec<_>>()
            .join(",")
    }
}

#[derive(Default)]
pub struct DryRunLinuxCommandRunner;

impl LinuxCommandRunner for DryRunLinuxCommandRunner {
    fn runner_name(&self) -> &str {
        "dry-run-command-runner"
    }

    fn run(&mut self, program: &str, args: &[String]) -> Result<String, String> {
        Ok(format!("dry_run:{}", command_line(program, args)))
    }
}

#[derive(Default)]
pub struct SystemLinuxCommandRunner;

#[cfg(target_os = "linux")]
impl LinuxCommandRunner for SystemLinuxCommandRunner {
    fn runner_name(&self) -> &str {
        "system-command-runner"
    }

    fn run(&mut self, program: &str, args: &[String]) -> Result<String, String> {
        let output = std::process::Command::new(program)
            .args(args)
            .output()
            .map_err(|error| format!("failed to start `{program}`: {error}"))?;

        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if stdout.is_empty() {
                Ok(format!("{program} {}", args.join(" ")).trim().to_string())
            } else {
                Ok(stdout)
            }
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            if stderr.is_empty() {
                Err(format!("`{program}` exited with status {}", output.status))
            } else {
                Err(stderr)
            }
        }
    }
}

#[cfg(not(target_os = "linux"))]
impl LinuxCommandRunner for SystemLinuxCommandRunner {
    fn runner_name(&self) -> &str {
        "system-command-runner"
    }

    fn run(&mut self, program: &str, _args: &[String]) -> Result<String, String> {
        Err(format!(
            "`{program}` command execution is only available on Linux"
        ))
    }
}

#[derive(Default)]
pub struct UnconfirmedLinuxCommandRunner;

impl LinuxCommandRunner for UnconfirmedLinuxCommandRunner {
    fn runner_name(&self) -> &str {
        "unconfirmed-command-runner"
    }

    fn run(&mut self, program: &str, _args: &[String]) -> Result<String, String> {
        Err(format!(
            "`{program}` requires LiveLinuxCommandGuard and explicit confirmation"
        ))
    }
}

fn command_line(program: &str, args: &[String]) -> String {
    std::iter::once(program.to_string())
        .chain(args.iter().cloned())
        .collect::<Vec<_>>()
        .join(" ")
}

fn validate_command_target_pid(target_pid: u32) -> Result<(), String> {
    if target_pid == 0 {
        Err("refusing to apply Linux command to pid 0".to_string())
    } else {
        Ok(())
    }
}

fn captured_nice(captured_state: &LinuxCapturedState) -> Result<i32, String> {
    let state = captured_state
        .nice
        .as_ref()
        .ok_or_else(|| "missing original nice state".to_string())?;

    if !state.captured {
        return Err("original nice state was not captured".to_string());
    }

    state
        .original_nice
        .ok_or_else(|| "missing original nice value".to_string())
}

fn captured_affinity(captured_state: &LinuxCapturedState) -> Result<&[u32], String> {
    let state = captured_state
        .affinity
        .as_ref()
        .ok_or_else(|| "missing original affinity state".to_string())?;

    if !state.captured {
        return Err("original affinity state was not captured".to_string());
    }
    if state.original_cpus.is_empty() {
        return Err("missing original affinity cpu list".to_string());
    }

    Ok(state.original_cpus.as_slice())
}

pub struct CommandLinuxSyscallApplier {
    runner: Box<dyn LinuxCommandRunner>,
    live_guard: Option<LiveLinuxCommandGuard>,
}

impl Default for CommandLinuxSyscallApplier {
    fn default() -> Self {
        Self::new()
    }
}

impl CommandLinuxSyscallApplier {
    pub fn new() -> Self {
        Self::with_runner(UnconfirmedLinuxCommandRunner)
    }

    pub fn dry_run() -> Self {
        Self::with_runner(DryRunLinuxCommandRunner)
    }

    pub(crate) fn with_runner<R>(runner: R) -> Self
    where
        R: LinuxCommandRunner + 'static,
    {
        Self {
            runner: Box::new(runner),
            live_guard: None,
        }
    }

    pub(crate) fn guarded_live<R>(runner: R, guard: LiveLinuxCommandGuard) -> Self
    where
        R: LinuxCommandRunner + 'static,
    {
        Self {
            runner: Box::new(runner),
            live_guard: Some(guard),
        }
    }

    pub fn live(guard: LiveLinuxCommandGuard) -> Self {
        Self::guarded_live(SystemLinuxCommandRunner, guard)
    }

    fn run_audited(&mut self, program: &str, args: &[String]) -> Result<String, String> {
        let runner_name = self.runner.runner_name().to_string();
        let command = command_line(program, args);

        match self.runner.run(program, args) {
            Ok(output) => Ok(format!(
                "runner={runner_name};command={command};output={output}"
            )),
            Err(error) => Err(format!(
                "runner={runner_name};command={command};error={error}"
            )),
        }
    }

    fn live_guard(&self) -> Option<&LiveLinuxCommandGuard> {
        self.live_guard.as_ref()
    }
}

impl LinuxSyscallApplier for CommandLinuxSyscallApplier {
    fn applier_name(&self) -> &str {
        "command"
    }

    fn apply_operation(
        &mut self,
        target_pid: u32,
        operation: &LinuxSyscallOperation,
        captured_state: &LinuxCapturedState,
        _now_ms: u64,
    ) -> Result<String, String> {
        validate_command_target_pid(target_pid)?;
        if let Some(guard) = self.live_guard() {
            guard.validate_target(target_pid)?;
            if !guard.allows_operation(operation) {
                return Ok(guard.skipped_detail(operation));
            }
        }

        match operation {
            LinuxSyscallOperation::SetNice { delta } => {
                let original_nice = captured_nice(captured_state)?;
                let requested_nice = (original_nice + delta).clamp(-20, 19);
                let (target_nice, priority_limited) = self
                    .live_guard()
                    .map(|guard| guard.bounded_live_nice_target(original_nice, requested_nice))
                    .unwrap_or((requested_nice, false));
                let detail = self.run_audited(
                    "renice",
                    &[
                        target_nice.to_string(),
                        "-p".to_string(),
                        target_pid.to_string(),
                    ],
                )?;
                if priority_limited {
                    Ok(format!("{detail};priority_raise_limited=true;requested_nice={requested_nice};applied_nice={target_nice}"))
                } else {
                    Ok(detail)
                }
            }
            LinuxSyscallOperation::SetAffinity {
                strategy,
                max_cpu_ratio,
            } => {
                let original_cpus = captured_affinity(captured_state)?;
                let target_cpus = CpuAffinityPlanner::default()
                    .plan_apply_target(strategy, *max_cpu_ratio, original_cpus)?
                    .to_taskset_list();
                self.run_audited(
                    "taskset",
                    &["-pc".to_string(), target_cpus, target_pid.to_string()],
                )
            }
            LinuxSyscallOperation::UseCpuset { enabled } => {
                if *enabled {
                    Err("cpuset command application is not implemented yet".to_string())
                } else {
                    Ok("cpuset disabled by policy".to_string())
                }
            }
            LinuxSyscallOperation::WarmupExecutor => Ok("warmup executor deferred".to_string()),
            LinuxSyscallOperation::RestoreNice => {
                let original_nice = captured_nice(captured_state)?;
                self.run_audited(
                    "renice",
                    &[
                        original_nice.to_string(),
                        "-p".to_string(),
                        target_pid.to_string(),
                    ],
                )
            }
            LinuxSyscallOperation::RestoreAffinity => {
                let original_cpus = captured_affinity(captured_state)?;
                let target_cpus = CpuAffinityPlanner::default()
                    .plan_rollback_target(original_cpus)?
                    .to_taskset_list();
                self.run_audited(
                    "taskset",
                    &["-pc".to_string(), target_cpus, target_pid.to_string()],
                )
            }
            LinuxSyscallOperation::RestoreCpuset => {
                let original_cpuset = captured_state
                    .cpuset
                    .as_ref()
                    .and_then(|state| state.original_cpuset.as_ref())
                    .cloned()
                    .ok_or_else(|| "missing original cpuset state".to_string())?;
                Err(format!(
                    "cpuset restore requires cgroup write support for `{original_cpuset}`"
                ))
            }
            LinuxSyscallOperation::NoopWarmupRollback => Ok("warmup rollback noop".to_string()),
        }
    }

    fn rollback_operation(
        &mut self,
        target_pid: u32,
        operation: &LinuxSyscallOperation,
        captured_state: &LinuxCapturedState,
        now_ms: u64,
    ) -> Result<String, String> {
        self.apply_operation(target_pid, operation, captured_state, now_ms)
    }
}

#[derive(Default)]
pub struct UnavailableLinuxProcessStateProvider;

impl LinuxProcessStateProvider for UnavailableLinuxProcessStateProvider {
    fn provider_name(&self) -> &str {
        "unavailable"
    }

    fn capture_nice(&self, _pid: u32) -> LinuxNiceState {
        LinuxNiceState::default()
    }

    fn capture_affinity(&self, _pid: u32) -> LinuxAffinityState {
        LinuxAffinityState::default()
    }

    fn capture_cpuset(&self, _pid: u32) -> LinuxCpusetState {
        LinuxCpusetState::default()
    }
}

#[derive(Default)]
pub struct PlannedLinuxSyscallApplier;

impl LinuxSyscallApplier for PlannedLinuxSyscallApplier {
    fn applier_name(&self) -> &str {
        "planned-applier"
    }

    fn apply_operation(
        &mut self,
        target_pid: u32,
        operation: &LinuxSyscallOperation,
        _captured_state: &LinuxCapturedState,
        _now_ms: u64,
    ) -> Result<String, String> {
        Ok(format!(
            "planned_apply:{target_pid}:{}",
            linux_syscall_descriptor(operation)
        ))
    }

    fn rollback_operation(
        &mut self,
        target_pid: u32,
        operation: &LinuxSyscallOperation,
        _captured_state: &LinuxCapturedState,
        _now_ms: u64,
    ) -> Result<String, String> {
        Ok(format!(
            "planned_rollback:{target_pid}:{}",
            linux_syscall_descriptor(operation)
        ))
    }
}

#[cfg(target_os = "linux")]
pub struct ProcfsLinuxProcessStateProvider;

#[cfg(target_os = "linux")]
impl Default for ProcfsLinuxProcessStateProvider {
    fn default() -> Self {
        Self
    }
}

#[cfg(target_os = "linux")]
impl LinuxProcessStateProvider for ProcfsLinuxProcessStateProvider {
    fn provider_name(&self) -> &str {
        "procfs"
    }

    fn capture_nice(&self, pid: u32) -> LinuxNiceState {
        match std::fs::read_to_string(format!("/proc/{pid}/stat")) {
            Ok(raw) => parse_proc_stat_nice(&raw)
                .map(|original_nice| LinuxNiceState {
                    captured: true,
                    original_nice: Some(original_nice),
                })
                .unwrap_or_default(),
            Err(_) => LinuxNiceState::default(),
        }
    }

    fn capture_affinity(&self, pid: u32) -> LinuxAffinityState {
        match std::fs::read_to_string(format!("/proc/{pid}/status")) {
            Ok(raw) => parse_status_cpu_list(&raw)
                .and_then(|configured_cpus| {
                    CpuAffinityPlanner::discover().plan_capture(configured_cpus)
                })
                .map(|capture| LinuxAffinityState {
                    captured: true,
                    original_cpus: capture.allowed_cpus,
                })
                .unwrap_or_default(),
            Err(_) => LinuxAffinityState::default(),
        }
    }

    fn capture_cpuset(&self, pid: u32) -> LinuxCpusetState {
        match std::fs::read_to_string(format!("/proc/{pid}/cpuset")) {
            Ok(raw) => {
                let cpuset = raw.trim().to_string();
                LinuxCpusetState {
                    captured: true,
                    was_enabled: Some(!cpuset.is_empty() && cpuset != "/"),
                    original_cpuset: (!cpuset.is_empty()).then_some(cpuset),
                }
            }
            Err(_) => LinuxCpusetState::default(),
        }
    }
}

#[cfg(not(target_os = "linux"))]
#[derive(Default)]
pub struct ProcfsLinuxProcessStateProvider;

#[cfg(not(target_os = "linux"))]
impl LinuxProcessStateProvider for ProcfsLinuxProcessStateProvider {
    fn provider_name(&self) -> &str {
        "procfs-unavailable"
    }

    fn capture_nice(&self, _pid: u32) -> LinuxNiceState {
        LinuxNiceState::default()
    }

    fn capture_affinity(&self, _pid: u32) -> LinuxAffinityState {
        LinuxAffinityState::default()
    }

    fn capture_cpuset(&self, _pid: u32) -> LinuxCpusetState {
        LinuxCpusetState::default()
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LinuxRollbackReport {
    pub restored: Vec<String>,
    pub skipped: Vec<String>,
    pub missing_state: Vec<String>,
    pub failed: Vec<String>,
}

impl LinuxRollbackReport {
    fn into_execution(self, mut execution: BackendExecution) -> BackendExecution {
        if !self.restored.is_empty() {
            execution = execution.with_field("rollback.restored", self.restored.join(","));
        }
        if !self.skipped.is_empty() {
            execution = execution.with_field("rollback.skipped", self.skipped.join(","));
        }
        if !self.missing_state.is_empty() {
            execution =
                execution.with_field("rollback.missing_state", self.missing_state.join(","));
        }
        if !self.failed.is_empty() {
            execution = execution.with_field("rollback.failed", self.failed.join(","));
        }

        execution
    }
}

pub trait LinuxSyscallExecutor {
    fn executor_name(&self) -> &str;

    fn execute_apply(&mut self, plan: &LinuxSyscallPlan, now_ms: u64) -> BackendApplyResult;

    fn execute_rollback(
        &mut self,
        plan: &LinuxSyscallPlan,
        lease: Option<&BackendLease>,
        now_ms: u64,
    ) -> BackendExecution;
}

pub trait ActuatorBackend {
    fn backend_name(&self) -> &str;

    fn apply(&mut self, plan: &ActionPlan, now_ms: u64) -> BackendApplyResult;

    fn rollback(
        &mut self,
        applied: &AppliedAction,
        lease: Option<&BackendLease>,
        now_ms: u64,
    ) -> BackendExecution;
}

#[derive(Default)]
pub struct NoopActuatorBackend;

impl ActuatorBackend for NoopActuatorBackend {
    fn backend_name(&self) -> &str {
        "noop"
    }

    fn apply(&mut self, plan: &ActionPlan, now_ms: u64) -> BackendApplyResult {
        BackendApplyResult {
            execution: BackendExecution::default()
                .with_field("backend", self.backend_name())
                .with_field("mode", "simulated")
                .with_field("timestamp_ms", now_ms.to_string())
                .with_field("action_count", plan.actions.len().to_string()),
            lease: Some(
                BackendLease::new(self.backend_name())
                    .with_field("mode", "simulated")
                    .with_field("target_pid", plan.target_pid.to_string())
                    .with_field("action_count", plan.actions.len().to_string()),
            ),
        }
    }

    fn rollback(
        &mut self,
        applied: &AppliedAction,
        lease: Option<&BackendLease>,
        now_ms: u64,
    ) -> BackendExecution {
        let mut execution = BackendExecution::default()
            .with_field("backend", self.backend_name())
            .with_field("mode", "simulated")
            .with_field("timestamp_ms", now_ms.to_string())
            .with_field("action_count", applied.actions.len().to_string());

        if let Some(lease) = lease {
            execution = execution.with_field("lease_backend", lease.backend_name.clone());
        }

        execution
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BackendOperationKind {
    Apply,
    Rollback,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BackendOperation {
    pub kind: BackendOperationKind,
    pub scenario: ScenarioKind,
    pub target_pid: u32,
    pub action_count: usize,
    pub timestamp_ms: u64,
}

#[derive(Default)]
pub struct RecordingActuatorBackend {
    operations: Vec<BackendOperation>,
}

impl RecordingActuatorBackend {
    pub fn operations(&self) -> &[BackendOperation] {
        &self.operations
    }
}

impl ActuatorBackend for RecordingActuatorBackend {
    fn backend_name(&self) -> &str {
        "recording"
    }

    fn apply(&mut self, plan: &ActionPlan, now_ms: u64) -> BackendApplyResult {
        self.operations.push(BackendOperation {
            kind: BackendOperationKind::Apply,
            scenario: plan.scenario.clone(),
            target_pid: plan.target_pid,
            action_count: plan.actions.len(),
            timestamp_ms: now_ms,
        });

        BackendApplyResult {
            execution: BackendExecution::default()
                .with_field("backend", self.backend_name())
                .with_field("mode", "recorded")
                .with_field("operation_index", self.operations.len().to_string()),
            lease: Some(
                BackendLease::new(self.backend_name())
                    .with_field("mode", "recorded")
                    .with_field("operation_index", self.operations.len().to_string()),
            ),
        }
    }

    fn rollback(
        &mut self,
        applied: &AppliedAction,
        _lease: Option<&BackendLease>,
        now_ms: u64,
    ) -> BackendExecution {
        self.operations.push(BackendOperation {
            kind: BackendOperationKind::Rollback,
            scenario: applied.scenario.clone(),
            target_pid: applied.target_pid,
            action_count: applied.actions.len(),
            timestamp_ms: now_ms,
        });

        BackendExecution::default()
            .with_field("backend", self.backend_name())
            .with_field("mode", "recorded")
            .with_field("operation_index", self.operations.len().to_string())
    }
}

pub struct PlannedOnlyLinuxSyscallExecutor {
    state_provider: Box<dyn LinuxProcessStateProvider>,
    applier: Box<dyn LinuxSyscallApplier>,
    live_guard: Option<LiveLinuxCommandGuard>,
}

impl Default for PlannedOnlyLinuxSyscallExecutor {
    fn default() -> Self {
        Self::new()
    }
}

impl PlannedOnlyLinuxSyscallExecutor {
    pub fn new() -> Self {
        Self::with_state_provider_and_applier(
            ProcfsLinuxProcessStateProvider,
            PlannedLinuxSyscallApplier,
        )
    }

    pub fn with_state_provider<P>(state_provider: P) -> Self
    where
        P: LinuxProcessStateProvider + 'static,
    {
        Self::with_state_provider_and_applier(state_provider, PlannedLinuxSyscallApplier)
    }

    pub fn with_state_provider_and_applier<P, A>(state_provider: P, applier: A) -> Self
    where
        P: LinuxProcessStateProvider + 'static,
        A: LinuxSyscallApplier + 'static,
    {
        Self {
            state_provider: Box::new(state_provider),
            applier: Box::new(applier),
            live_guard: None,
        }
    }

    pub fn with_live_guard(mut self, guard: LiveLinuxCommandGuard) -> Self {
        self.live_guard = Some(guard);
        self
    }
}

impl LinuxSyscallExecutor for PlannedOnlyLinuxSyscallExecutor {
    fn executor_name(&self) -> &str {
        "planned-only"
    }

    fn execute_apply(&mut self, plan: &LinuxSyscallPlan, now_ms: u64) -> BackendApplyResult {
        let mut execution = BackendExecution::default()
            .with_field("executor", self.executor_name())
            .with_field("mode", "planned_only")
            .with_field("phase", phase_name(plan.phase))
            .with_field("timestamp_ms", now_ms.to_string())
            .with_field("target_pid", plan.target_pid.to_string());
        execution = annotate_live_guard(execution, plan.target_pid, self.live_guard.as_ref());

        for (index, operation) in plan.operations.iter().enumerate() {
            execution = execution.with_field(
                format!("syscall.{index}"),
                linux_syscall_descriptor(operation),
            );
        }

        let captured_state = capture_linux_state(plan, self.state_provider.as_ref());
        execution = annotate_captured_state(
            execution,
            self.state_provider.provider_name(),
            &captured_state,
        );
        execution = execution.with_field("applier", self.applier.applier_name());

        let mut applied_count = 0usize;
        let mut failed_operations = Vec::new();
        let mut skipped_count = 0usize;
        for (index, operation) in plan.operations.iter().enumerate() {
            if let Some(guard) = self.live_guard.as_ref() {
                if !guard.target_allowed(plan.target_pid) {
                    let error = guard
                        .validate_target(plan.target_pid)
                        .err()
                        .unwrap_or_else(|| {
                            "linux-command live actuator target rejected".to_string()
                        });
                    failed_operations.push(format!(
                        "{index}:{}:{error}",
                        linux_syscall_descriptor(operation)
                    ));
                    execution = execution
                        .with_field(format!("apply.{index}.status"), "error")
                        .with_field(format!("apply.{index}.error"), error);
                    continue;
                }
                if !guard.allows_operation(operation) {
                    skipped_count += 1;
                    execution = execution
                        .with_field(format!("apply.{index}.status"), "skipped")
                        .with_field(
                            format!("apply.{index}.detail"),
                            guard.skipped_detail(operation),
                        );
                    continue;
                }
            }

            match self
                .applier
                .apply_operation(plan.target_pid, operation, &captured_state, now_ms)
            {
                Ok(detail) => {
                    applied_count += 1;
                    execution = execution
                        .with_field(format!("apply.{index}.status"), "ok")
                        .with_field(format!("apply.{index}.detail"), detail);
                }
                Err(error) => {
                    failed_operations.push(format!(
                        "{index}:{}:{error}",
                        linux_syscall_descriptor(operation)
                    ));
                    execution = execution
                        .with_field(format!("apply.{index}.status"), "error")
                        .with_field(format!("apply.{index}.error"), error);
                }
            }
        }
        execution = execution
            .with_field("apply.attempted_count", plan.operations.len().to_string())
            .with_field("apply.applied_count", applied_count.to_string())
            .with_field("apply.skipped_count", skipped_count.to_string())
            .with_field("apply.failed_count", failed_operations.len().to_string())
            .with_field(
                "apply.partial",
                (applied_count > 0 && !failed_operations.is_empty()).to_string(),
            )
            .with_field(
                "apply.result",
                if failed_operations.is_empty() {
                    "ok"
                } else if applied_count == 0 {
                    "error"
                } else {
                    "partial"
                },
            );
        if !failed_operations.is_empty() {
            execution = execution.with_field("apply.failed", failed_operations.join(","));
        }

        let lease = captured_state.apply_to_lease(
            BackendLease::new(self.executor_name())
                .with_field("rollback_strategy", "linux_syscall_restore")
                .with_field("target_pid", plan.target_pid.to_string())
                .with_field(
                    "linux.capture.provider",
                    self.state_provider.provider_name(),
                )
                .with_field("linux.applier", self.applier.applier_name()),
        );
        let lease = if let Some(guard) = self.live_guard.as_ref() {
            lease
                .with_field(
                    "linux.live_guard.confirmed",
                    guard.explicit_confirmation.to_string(),
                )
                .with_field("linux.live_guard.scope", guard.scope_label())
                .with_field("linux.live_guard.allowed_pids", guard.allowed_pids_label())
                .with_field(
                    "linux.live_guard.priority_raise_allowed",
                    guard.allows_priority_raise().to_string(),
                )
        } else {
            lease
        };

        BackendApplyResult {
            execution,
            lease: Some(lease),
        }
    }

    fn execute_rollback(
        &mut self,
        plan: &LinuxSyscallPlan,
        lease: Option<&BackendLease>,
        now_ms: u64,
    ) -> BackendExecution {
        let captured_state = LinuxCapturedState::from_lease(lease);
        let mut execution = BackendExecution::default()
            .with_field("executor", self.executor_name())
            .with_field("mode", "planned_only")
            .with_field("phase", phase_name(plan.phase))
            .with_field("timestamp_ms", now_ms.to_string())
            .with_field("target_pid", plan.target_pid.to_string());
        execution = annotate_live_guard(execution, plan.target_pid, self.live_guard.as_ref());

        for (index, operation) in plan.operations.iter().enumerate() {
            execution = execution.with_field(
                format!("syscall.{index}"),
                linux_syscall_descriptor(operation),
            );
        }

        if let Some(lease) = lease {
            for (key, value) in &lease.captured_state {
                execution = execution.with_field(format!("lease.{key}"), value.clone());
            }
        }

        execution = execution.with_field("applier", self.applier.applier_name());
        execute_linux_rollbacks(
            &mut *self.applier,
            execution,
            plan,
            &captured_state,
            now_ms,
            self.live_guard.as_ref(),
        )
    }
}

pub struct LinuxActuatorBackend {
    backend_name: String,
    executor: Box<dyn LinuxSyscallExecutor>,
}

impl Default for LinuxActuatorBackend {
    fn default() -> Self {
        Self::with_executor(PlannedOnlyLinuxSyscallExecutor::default())
    }
}

impl LinuxActuatorBackend {
    pub fn with_executor<E>(executor: E) -> Self
    where
        E: LinuxSyscallExecutor + 'static,
    {
        Self::with_named_executor("linux-skeleton", executor)
    }

    pub fn with_named_executor<E>(backend_name: impl Into<String>, executor: E) -> Self
    where
        E: LinuxSyscallExecutor + 'static,
    {
        Self {
            backend_name: backend_name.into(),
            executor: Box::new(executor),
        }
    }
}

impl ActuatorBackend for LinuxActuatorBackend {
    fn backend_name(&self) -> &str {
        &self.backend_name
    }

    fn apply(&mut self, plan: &ActionPlan, now_ms: u64) -> BackendApplyResult {
        let syscall_plan = build_linux_apply_plan(plan);
        let mut result = self.executor.execute_apply(&syscall_plan, now_ms);
        result.execution = result
            .execution
            .with_field("backend", self.backend_name())
            .with_field("syscall_executor", self.executor.executor_name());
        result
    }

    fn rollback(
        &mut self,
        applied: &AppliedAction,
        lease: Option<&BackendLease>,
        now_ms: u64,
    ) -> BackendExecution {
        let syscall_plan = build_linux_rollback_plan(applied);
        self.executor
            .execute_rollback(&syscall_plan, lease, now_ms)
            .with_field("backend", self.backend_name())
            .with_field("syscall_executor", self.executor.executor_name())
    }
}

fn build_linux_apply_plan(plan: &ActionPlan) -> LinuxSyscallPlan {
    LinuxSyscallPlan {
        phase: LinuxSyscallPhase::Apply,
        target_pid: plan.target_pid,
        operations: plan
            .actions
            .iter()
            .map(|action| match action {
                Action::RaiseNice { delta } => LinuxSyscallOperation::SetNice { delta: *delta },
                Action::SetAffinity {
                    strategy,
                    max_cpu_ratio,
                } => LinuxSyscallOperation::SetAffinity {
                    strategy: strategy.as_str().to_string(),
                    max_cpu_ratio: *max_cpu_ratio,
                },
                Action::UseCpuset { enabled } => {
                    LinuxSyscallOperation::UseCpuset { enabled: *enabled }
                }
                Action::WarmupExecutor => LinuxSyscallOperation::WarmupExecutor,
            })
            .collect(),
    }
}

fn build_linux_rollback_plan(applied: &AppliedAction) -> LinuxSyscallPlan {
    LinuxSyscallPlan {
        phase: LinuxSyscallPhase::Rollback,
        target_pid: applied.target_pid,
        operations: applied
            .actions
            .iter()
            .filter_map(|action| match action {
                Action::RaiseNice { .. } => Some(LinuxSyscallOperation::RestoreNice),
                Action::SetAffinity { .. } => Some(LinuxSyscallOperation::RestoreAffinity),
                Action::UseCpuset { enabled } => {
                    enabled.then_some(LinuxSyscallOperation::RestoreCpuset)
                }
                Action::WarmupExecutor => Some(LinuxSyscallOperation::NoopWarmupRollback),
            })
            .collect(),
    }
}

fn phase_name(phase: LinuxSyscallPhase) -> &'static str {
    match phase {
        LinuxSyscallPhase::Apply => "apply",
        LinuxSyscallPhase::Rollback => "rollback",
    }
}

fn capture_linux_state(
    plan: &LinuxSyscallPlan,
    provider: &dyn LinuxProcessStateProvider,
) -> LinuxCapturedState {
    let mut state = LinuxCapturedState::default();

    for operation in &plan.operations {
        match operation {
            LinuxSyscallOperation::SetNice { .. } => {
                state
                    .nice
                    .get_or_insert_with(|| provider.capture_nice(plan.target_pid));
            }
            LinuxSyscallOperation::SetAffinity { .. } => {
                state
                    .affinity
                    .get_or_insert_with(|| provider.capture_affinity(plan.target_pid));
            }
            LinuxSyscallOperation::UseCpuset { enabled: true } => {
                state
                    .cpuset
                    .get_or_insert_with(|| provider.capture_cpuset(plan.target_pid));
            }
            LinuxSyscallOperation::UseCpuset { enabled: false } => {}
            LinuxSyscallOperation::WarmupExecutor
            | LinuxSyscallOperation::RestoreNice
            | LinuxSyscallOperation::RestoreAffinity
            | LinuxSyscallOperation::RestoreCpuset
            | LinuxSyscallOperation::NoopWarmupRollback => {}
        }
    }

    state
}

fn annotate_captured_state(
    mut execution: BackendExecution,
    provider_name: &str,
    captured_state: &LinuxCapturedState,
) -> BackendExecution {
    execution = execution.with_field("capture.provider", provider_name);

    if let Some(nice) = &captured_state.nice {
        execution = execution
            .with_field("capture.nice.captured", nice.captured.to_string())
            .with_field(
                "capture.nice.original",
                nice.original_nice
                    .map(|value| value.to_string())
                    .unwrap_or_default(),
            );
    }

    if let Some(affinity) = &captured_state.affinity {
        execution = execution
            .with_field("capture.affinity.captured", affinity.captured.to_string())
            .with_field(
                "capture.affinity.original_cpus",
                affinity
                    .original_cpus
                    .iter()
                    .map(u32::to_string)
                    .collect::<Vec<_>>()
                    .join(","),
            );
    }

    if let Some(cpuset) = &captured_state.cpuset {
        execution = execution
            .with_field("capture.cpuset.captured", cpuset.captured.to_string())
            .with_field(
                "capture.cpuset.original",
                cpuset.original_cpuset.clone().unwrap_or_default(),
            )
            .with_field(
                "capture.cpuset.was_enabled",
                cpuset
                    .was_enabled
                    .map(|value| value.to_string())
                    .unwrap_or_default(),
            );
    }

    execution
}

fn annotate_live_guard(
    mut execution: BackendExecution,
    target_pid: u32,
    live_guard: Option<&LiveLinuxCommandGuard>,
) -> BackendExecution {
    if let Some(guard) = live_guard {
        execution = execution
            .with_field(
                "live_guard.confirmed",
                guard.explicit_confirmation.to_string(),
            )
            .with_field("live_guard.scope", guard.scope_label())
            .with_field("live_guard.allowed_pids", guard.allowed_pids_label())
            .with_field(
                "live_guard.priority_raise_allowed",
                guard.allows_priority_raise().to_string(),
            )
            .with_field(
                "live_guard.target_allowed",
                guard.target_allowed(target_pid).to_string(),
            );
    }

    execution
}

fn rollback_component_name(operation: &LinuxSyscallOperation) -> Option<&'static str> {
    match operation {
        LinuxSyscallOperation::RestoreNice => Some("nice"),
        LinuxSyscallOperation::RestoreAffinity => Some("affinity"),
        LinuxSyscallOperation::RestoreCpuset => Some("cpuset"),
        LinuxSyscallOperation::NoopWarmupRollback => Some("warmup_executor"),
        LinuxSyscallOperation::SetNice { .. }
        | LinuxSyscallOperation::SetAffinity { .. }
        | LinuxSyscallOperation::UseCpuset { .. }
        | LinuxSyscallOperation::WarmupExecutor => None,
    }
}

fn execute_linux_rollbacks(
    applier: &mut dyn LinuxSyscallApplier,
    mut execution: BackendExecution,
    plan: &LinuxSyscallPlan,
    captured_state: &LinuxCapturedState,
    now_ms: u64,
    live_guard: Option<&LiveLinuxCommandGuard>,
) -> BackendExecution {
    let report = build_linux_rollback_report(
        applier,
        plan,
        captured_state,
        now_ms,
        &mut execution,
        live_guard,
    );
    report.into_execution(execution)
}

fn build_linux_rollback_report(
    applier: &mut dyn LinuxSyscallApplier,
    plan: &LinuxSyscallPlan,
    captured_state: &LinuxCapturedState,
    now_ms: u64,
    execution: &mut BackendExecution,
    live_guard: Option<&LiveLinuxCommandGuard>,
) -> LinuxRollbackReport {
    let mut report = LinuxRollbackReport::default();

    for (index, operation) in plan.operations.iter().enumerate() {
        if let Some(guard) = live_guard {
            if !guard.target_allowed(plan.target_pid) {
                let error = guard
                    .validate_target(plan.target_pid)
                    .err()
                    .unwrap_or_else(|| "linux-command live actuator target rejected".to_string());
                report.failed.push(format!(
                    "{}:{error}",
                    rollback_component_name(operation).unwrap_or("operation")
                ));
                *execution = std::mem::take(execution)
                    .with_field(format!("rollback.{index}.status"), "error")
                    .with_field(format!("rollback.{index}.error"), error);
                continue;
            }
            if !guard.allows_operation(operation) {
                report.skipped.push(
                    rollback_component_name(operation)
                        .unwrap_or("operation")
                        .to_string(),
                );
                *execution = std::mem::take(execution)
                    .with_field(format!("rollback.{index}.status"), "skipped")
                    .with_field(
                        format!("rollback.{index}.detail"),
                        guard.skipped_detail(operation),
                    );
                continue;
            }
        }

        match operation {
            LinuxSyscallOperation::RestoreNice => {
                match captured_state.nice.as_ref().map(|state| state.captured) {
                    Some(true) => match applier.rollback_operation(
                        plan.target_pid,
                        operation,
                        captured_state,
                        now_ms,
                    ) {
                        Ok(detail) => {
                            report.restored.push("nice".to_string());
                            *execution = std::mem::take(execution)
                                .with_field(format!("rollback.{index}.status"), "ok")
                                .with_field(format!("rollback.{index}.detail"), detail);
                        }
                        Err(error) => {
                            report.failed.push(format!("nice:{error}"));
                            *execution = std::mem::take(execution)
                                .with_field(format!("rollback.{index}.status"), "error")
                                .with_field(format!("rollback.{index}.error"), error);
                        }
                    },
                    _ => {
                        report.missing_state.push("nice".to_string());
                        *execution = std::mem::take(execution)
                            .with_field(format!("rollback.{index}.status"), "missing_state")
                            .with_field(
                                format!("rollback.{index}.detail"),
                                "missing original nice state; rollback skipped",
                            );
                    }
                }
            }
            LinuxSyscallOperation::RestoreAffinity => {
                match captured_state.affinity.as_ref().map(|state| state.captured) {
                    Some(true) => match applier.rollback_operation(
                        plan.target_pid,
                        operation,
                        captured_state,
                        now_ms,
                    ) {
                        Ok(detail) => {
                            report.restored.push("affinity".to_string());
                            *execution = std::mem::take(execution)
                                .with_field(format!("rollback.{index}.status"), "ok")
                                .with_field(format!("rollback.{index}.detail"), detail);
                        }
                        Err(error) => {
                            report.failed.push(format!("affinity:{error}"));
                            *execution = std::mem::take(execution)
                                .with_field(format!("rollback.{index}.status"), "error")
                                .with_field(format!("rollback.{index}.error"), error);
                        }
                    },
                    _ => {
                        report.missing_state.push("affinity".to_string());
                        *execution = std::mem::take(execution)
                            .with_field(format!("rollback.{index}.status"), "missing_state")
                            .with_field(
                                format!("rollback.{index}.detail"),
                                "missing original affinity state; rollback skipped",
                            );
                    }
                }
            }
            LinuxSyscallOperation::RestoreCpuset => {
                match captured_state.cpuset.as_ref().map(|state| state.captured) {
                    Some(true) => match applier.rollback_operation(
                        plan.target_pid,
                        operation,
                        captured_state,
                        now_ms,
                    ) {
                        Ok(detail) => {
                            report.restored.push("cpuset".to_string());
                            *execution = std::mem::take(execution)
                                .with_field(format!("rollback.{index}.status"), "ok")
                                .with_field(format!("rollback.{index}.detail"), detail);
                        }
                        Err(error) => {
                            report.failed.push(format!("cpuset:{error}"));
                            *execution = std::mem::take(execution)
                                .with_field(format!("rollback.{index}.status"), "error")
                                .with_field(format!("rollback.{index}.error"), error);
                        }
                    },
                    _ => {
                        report.missing_state.push("cpuset".to_string());
                        *execution = std::mem::take(execution)
                            .with_field(format!("rollback.{index}.status"), "missing_state")
                            .with_field(
                                format!("rollback.{index}.detail"),
                                "missing original cpuset state; rollback skipped",
                            );
                    }
                }
            }
            LinuxSyscallOperation::NoopWarmupRollback => {
                match applier.rollback_operation(plan.target_pid, operation, captured_state, now_ms)
                {
                    Ok(detail) => {
                        report.restored.push("warmup_executor".to_string());
                        *execution = std::mem::take(execution)
                            .with_field(format!("rollback.{index}.status"), "ok")
                            .with_field(format!("rollback.{index}.detail"), detail);
                    }
                    Err(error) => {
                        report.failed.push(format!("warmup_executor:{error}"));
                        *execution = std::mem::take(execution)
                            .with_field(format!("rollback.{index}.status"), "error")
                            .with_field(format!("rollback.{index}.error"), error);
                    }
                }
            }
            LinuxSyscallOperation::SetNice { .. }
            | LinuxSyscallOperation::SetAffinity { .. }
            | LinuxSyscallOperation::UseCpuset { .. }
            | LinuxSyscallOperation::WarmupExecutor => {}
        }
    }

    report
}

fn linux_syscall_descriptor(operation: &LinuxSyscallOperation) -> String {
    match operation {
        LinuxSyscallOperation::SetNice { delta } => format!("set_nice:{delta}"),
        LinuxSyscallOperation::SetAffinity {
            strategy,
            max_cpu_ratio,
        } => format!("set_affinity:{strategy}:{max_cpu_ratio}"),
        LinuxSyscallOperation::UseCpuset { enabled } => format!("use_cpuset:{enabled}"),
        LinuxSyscallOperation::WarmupExecutor => "warmup_executor".to_string(),
        LinuxSyscallOperation::RestoreNice => "restore_nice".to_string(),
        LinuxSyscallOperation::RestoreAffinity => "restore_affinity".to_string(),
        LinuxSyscallOperation::RestoreCpuset => "restore_cpuset".to_string(),
        LinuxSyscallOperation::NoopWarmupRollback => "noop_warmup_rollback".to_string(),
    }
}

#[cfg(target_os = "linux")]
fn parse_proc_stat_nice(raw: &str) -> Option<i32> {
    let stat_fields = raw.rsplit_once(") ")?.1;
    stat_fields
        .split_whitespace()
        .nth(16)
        .and_then(|value| value.parse::<i32>().ok())
}
