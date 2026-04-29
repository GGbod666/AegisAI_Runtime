use std::collections::{BTreeMap, HashMap};

use crate::backend::{ActuatorBackend, BackendLease, LinuxActuatorBackend, NoopActuatorBackend};
use crate::model::{Action, ActionPlan, AppliedAction, AppliedActionState, ScenarioKind};

pub struct Actuator {
    backend: Box<dyn ActuatorBackend>,
    active_actions: HashMap<(u32, ScenarioKind), ActionLease>,
}

impl Default for Actuator {
    fn default() -> Self {
        Self::with_backend(NoopActuatorBackend)
    }
}

impl Actuator {
    pub fn with_backend<B>(backend: B) -> Self
    where
        B: ActuatorBackend + 'static,
    {
        Self {
            backend: Box::new(backend),
            active_actions: HashMap::new(),
        }
    }

    pub fn with_noop_backend() -> Self {
        Self::with_backend(NoopActuatorBackend)
    }

    pub fn with_linux_backend() -> Self {
        Self::with_backend(LinuxActuatorBackend::default())
    }

    pub fn backend_name(&self) -> &str {
        self.backend.backend_name()
    }

    pub fn apply(&mut self, plan: ActionPlan, now_ms: u64, require_revert: bool) -> AppliedAction {
        let expires_at_ms = if require_revert {
            now_ms.saturating_add(plan.duration_ms)
        } else {
            now_ms
        };
        let backend_apply = self.backend.apply(&plan, now_ms);
        let mut audit_fields = plan.audit_fields.clone();
        merge_prefixed_fields(
            &mut audit_fields,
            "backend.apply",
            &backend_apply.execution.audit_fields,
        );

        let applied = AppliedAction {
            scenario: plan.scenario.clone(),
            target_pid: plan.target_pid,
            target_process_name: plan.target_process_name.clone(),
            actions: plan.actions.clone(),
            applied_at_ms: now_ms,
            expires_at_ms,
            state: AppliedActionState::Applied,
            audit_fields: audit_fields.clone(),
        };

        if require_revert {
            self.active_actions.insert(
                (plan.target_pid, plan.scenario),
                ActionLease {
                    target_process_name: plan.target_process_name,
                    actions: plan.actions,
                    applied_at_ms: now_ms,
                    expires_at_ms,
                    audit_fields,
                    backend_lease: backend_apply.lease,
                },
            );
        }

        applied
    }

    pub fn expire(&mut self, now_ms: u64) -> Vec<AppliedAction> {
        let mut expired_keys = self
            .active_actions
            .iter()
            .filter_map(|(key, lease)| (lease.expires_at_ms <= now_ms).then_some(key.clone()))
            .collect::<Vec<_>>();
        expired_keys.sort_unstable_by(|left, right| {
            self.active_actions[left]
                .expires_at_ms
                .cmp(&self.active_actions[right].expires_at_ms)
                .then_with(|| left.0.cmp(&right.0))
                .then_with(|| left.1.cmp(&right.1))
        });

        let mut rollbacks = Vec::with_capacity(expired_keys.len());
        for key in expired_keys {
            if let Some(lease) = self.active_actions.remove(&key) {
                let provisional = AppliedAction {
                    scenario: key.1,
                    target_pid: key.0,
                    target_process_name: lease.target_process_name.clone(),
                    actions: lease.actions.clone(),
                    applied_at_ms: lease.applied_at_ms,
                    expires_at_ms: lease.expires_at_ms,
                    state: AppliedActionState::RolledBack,
                    audit_fields: lease.audit_fields.clone(),
                };
                let rollback_execution =
                    self.backend
                        .rollback(&provisional, lease.backend_lease.as_ref(), now_ms);
                let mut audit_fields = provisional.audit_fields.clone();
                merge_prefixed_fields(
                    &mut audit_fields,
                    "backend.rollback",
                    &rollback_execution.audit_fields,
                );
                rollbacks.push(AppliedAction {
                    audit_fields,
                    ..provisional
                });
            }
        }

        rollbacks
    }

    pub fn active_count(&self) -> usize {
        self.active_actions.len()
    }
}

struct ActionLease {
    target_process_name: String,
    actions: Vec<Action>,
    applied_at_ms: u64,
    expires_at_ms: u64,
    audit_fields: BTreeMap<String, String>,
    backend_lease: Option<BackendLease>,
}

fn merge_prefixed_fields(
    target: &mut BTreeMap<String, String>,
    prefix: &str,
    source: &BTreeMap<String, String>,
) {
    for (key, value) in source {
        target.insert(format!("{prefix}.{key}"), value.clone());
    }
}
