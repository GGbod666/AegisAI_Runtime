use std::collections::{BTreeMap, HashMap};

use crate::config::{SafetyConfig, ScenarioPolicy};
use crate::model::{Action, ActionPlan, PolicyContext, ScenarioKind, WorkloadTag};

pub struct PolicyEngine {
    policies: BTreeMap<ScenarioKind, ScenarioPolicy>,
    safety: SafetyConfig,
    last_triggered_at: HashMap<(u32, ScenarioKind), u64>,
}

impl PolicyEngine {
    pub fn new(policies: BTreeMap<ScenarioKind, ScenarioPolicy>, safety: SafetyConfig) -> Self {
        Self {
            policies,
            safety,
            last_triggered_at: HashMap::new(),
        }
    }

    pub fn enabled_policies_snapshot(&self) -> Vec<(ScenarioKind, ScenarioPolicy)> {
        self.policies
            .iter()
            .filter(|(_, policy)| policy.enabled)
            .map(|(scenario, policy)| (scenario.clone(), policy.clone()))
            .collect()
    }

    pub fn evaluate(&mut self, context: &PolicyContext) -> Option<ActionPlan> {
        let candidate = self.evaluate_candidate(context)?;
        self.record_trigger(
            candidate.plan.target_pid,
            &candidate.plan.scenario,
            candidate.triggered_at_ms,
        );
        Some(candidate.plan)
    }

    pub fn evaluate_all<'a, I>(&mut self, contexts: I) -> Vec<ActionPlan>
    where
        I: IntoIterator<Item = &'a PolicyContext>,
    {
        let mut candidates = Vec::new();

        for context in contexts {
            if let Some(candidate) = self.evaluate_candidate(context) {
                candidates.push(candidate);
            }
        }

        let resolved = self.resolve_conflicts(candidates);
        for candidate in &resolved {
            self.record_trigger(
                candidate.plan.target_pid,
                &candidate.plan.scenario,
                candidate.triggered_at_ms,
            );
        }

        resolved
            .into_iter()
            .map(|candidate| candidate.plan)
            .collect()
    }

    fn evaluate_candidate(&self, context: &PolicyContext) -> Option<TimedPlan> {
        let policy = self.policies.get(&context.scenario)?;
        if !policy.enabled {
            return None;
        }

        let cooldown_key = (context.event.pid, context.scenario.clone());
        if let Some(last_triggered_at) = self.last_triggered_at.get(&cooldown_key) {
            if context.event.timestamp_ms < last_triggered_at.saturating_add(policy.cooldown_ms) {
                return None;
            }
        }

        let plan = match context.scenario {
            ScenarioKind::InferenceTailGuard => {
                crate::scenarios::inference_tail_guard::evaluate(policy, &self.safety, context)?
            }
            ScenarioKind::ToolCallBooster => {
                crate::scenarios::tool_call_booster::evaluate(policy, &self.safety, context)?
            }
            _ => evaluate_generic_candidate(policy, &self.safety, context)?,
        };

        Some(TimedPlan {
            triggered_at_ms: context.event.timestamp_ms,
            plan,
        })
    }

    fn resolve_conflicts(&self, mut candidates: Vec<TimedPlan>) -> Vec<TimedPlan> {
        candidates.sort_by(|left, right| {
            left.plan
                .target_pid
                .cmp(&right.plan.target_pid)
                .then_with(|| {
                    right
                        .plan
                        .scenario
                        .priority()
                        .cmp(&left.plan.scenario.priority())
                })
                .then_with(|| {
                    left.plan
                        .scenario
                        .as_str()
                        .cmp(right.plan.scenario.as_str())
                })
        });

        let mut claimed_slots: HashMap<(u32, ActionSlot), ScenarioKind> = HashMap::new();
        let mut resolved = Vec::new();

        for mut candidate in candidates {
            let mut kept_actions = Vec::new();
            let mut suppressed = Vec::new();

            for action in candidate.plan.actions.drain(..) {
                let slot = ActionSlot::from_action(&action);
                let key = (candidate.plan.target_pid, slot);

                if let Some(winner) = claimed_slots.get(&key) {
                    suppressed.push(format!("{}=>{}", action_name(&action), winner.as_str()));
                    continue;
                }

                claimed_slots.insert(key, candidate.plan.scenario.clone());
                kept_actions.push(action);
            }

            if kept_actions.is_empty() {
                continue;
            }

            if !suppressed.is_empty() {
                candidate
                    .plan
                    .audit_fields
                    .insert("suppressed_actions".to_string(), suppressed.join(","));
            }

            candidate.plan.actions = kept_actions;
            resolved.push(candidate);
        }

        resolved.sort_by(|left, right| {
            left.plan
                .target_pid
                .cmp(&right.plan.target_pid)
                .then_with(|| {
                    left.plan
                        .scenario
                        .as_str()
                        .cmp(right.plan.scenario.as_str())
                })
        });

        resolved
    }

    fn record_trigger(&mut self, pid: u32, scenario: &ScenarioKind, timestamp_ms: u64) {
        self.last_triggered_at
            .insert((pid, scenario.clone()), timestamp_ms);
    }
}

struct TimedPlan {
    triggered_at_ms: u64,
    plan: ActionPlan,
}

fn evaluate_generic_candidate(
    policy: &ScenarioPolicy,
    safety: &SafetyConfig,
    context: &PolicyContext,
) -> Option<ActionPlan> {
    if !scenario_matches_profile(&context.scenario, &context.profile) {
        return None;
    }

    let breaches = trigger_breaches(policy, &context.feature_window);
    if breaches.is_empty() {
        return None;
    }

    let mut audit_fields = context.audit_fields.clone();
    let actions = build_actions(policy, safety, &mut audit_fields);
    if actions.is_empty() {
        return None;
    }

    audit_fields.insert(
        "scenario".to_string(),
        context.scenario.as_str().to_string(),
    );
    audit_fields.insert("pid".to_string(), context.event.pid.to_string());
    audit_fields.insert("breaches".to_string(), breaches.join(","));
    audit_fields.insert(
        "matched_rules".to_string(),
        context.profile.matched_rules.join(","),
    );

    Some(ActionPlan {
        scenario: context.scenario.clone(),
        target_pid: context.event.pid,
        target_process_name: context.event.process_name.clone(),
        actions,
        duration_ms: policy
            .max_boost_duration_ms
            .min(safety.max_boost_duration_ms),
        rationale: breaches,
        audit_fields,
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum ActionSlot {
    RaiseNice,
    SetAffinity,
    UseCpuset,
    WarmupExecutor,
}

impl ActionSlot {
    fn from_action(action: &Action) -> Self {
        match action {
            Action::RaiseNice { .. } => Self::RaiseNice,
            Action::SetAffinity { .. } => Self::SetAffinity,
            Action::UseCpuset { .. } => Self::UseCpuset,
            Action::WarmupExecutor => Self::WarmupExecutor,
        }
    }
}

fn build_actions(
    policy: &ScenarioPolicy,
    safety: &SafetyConfig,
    audit_fields: &mut BTreeMap<String, String>,
) -> Vec<Action> {
    let mut actions = Vec::new();

    if let Some(delta) = policy.actions.raise_nice {
        let bounded_delta = clamp_priority_delta(delta, safety.max_priority_delta);
        if bounded_delta != 0 {
            if bounded_delta != delta {
                audit_fields.insert(
                    "priority_delta_clamped".to_string(),
                    format!("{delta}->{bounded_delta}"),
                );
            }

            actions.push(Action::RaiseNice {
                delta: bounded_delta,
            });
        }
    }

    if let Some(strategy) = &policy.actions.pin_strategy {
        let max_cpu_ratio = safety.max_affinity_change_ratio.clamp(0.0, 1.0);
        if (max_cpu_ratio - safety.max_affinity_change_ratio).abs() > f32::EPSILON {
            audit_fields.insert(
                "affinity_ratio_clamped".to_string(),
                format!("{}->{max_cpu_ratio}", safety.max_affinity_change_ratio),
            );
        }

        actions.push(Action::SetAffinity {
            strategy: strategy.clone(),
            max_cpu_ratio,
        });
    }

    if let Some(use_cpuset) = policy.actions.use_cpuset {
        actions.push(Action::UseCpuset {
            enabled: use_cpuset,
        });
    }

    if policy.actions.warmup_executor.unwrap_or(false) {
        actions.push(Action::WarmupExecutor);
    }

    actions
}

fn clamp_priority_delta(delta: i32, max_priority_delta: i32) -> i32 {
    match delta.cmp(&0) {
        std::cmp::Ordering::Greater => delta.min(max_priority_delta),
        std::cmp::Ordering::Less => delta.max(-max_priority_delta),
        std::cmp::Ordering::Equal => 0,
    }
}

fn scenario_matches_profile(
    scenario: &ScenarioKind,
    profile: &crate::model::WorkloadProfile,
) -> bool {
    match scenario {
        ScenarioKind::InferenceTailGuard => {
            profile.has_tag(&WorkloadTag::AiInference)
                && profile.has_tag(&WorkloadTag::InteractiveLatencySensitive)
        }
        ScenarioKind::ToolCallBooster => profile.has_tag(&WorkloadTag::ToolCall),
        ScenarioKind::AiWorkloadAwareness => true,
        ScenarioKind::Unknown(_) => false,
    }
}

fn trigger_breaches(
    policy: &ScenarioPolicy,
    feature_window: &crate::model::FeatureWindow,
) -> Vec<String> {
    let mut breaches = Vec::new();

    if let Some(threshold) = policy.triggers.run_queue_delay_us {
        if feature_window.run_queue_delay_us_max >= threshold {
            breaches.push(format!(
                "run_queue_delay_us:{}>={threshold}",
                feature_window.run_queue_delay_us_max
            ));
        }
    }

    if let Some(threshold) = policy.triggers.offcpu_spike_us {
        if feature_window.offcpu_time_us_max >= threshold {
            breaches.push(format!(
                "offcpu_spike_us:{}>={threshold}",
                feature_window.offcpu_time_us_max
            ));
        }
    }

    if let Some(threshold) = policy.triggers.cpu_migrations_per_sec {
        if feature_window.cpu_migrations_per_sec >= threshold {
            breaches.push(format!(
                "cpu_migrations_per_sec:{}>={threshold}",
                feature_window.cpu_migrations_per_sec
            ));
        }
    }

    if let Some(threshold) = policy.triggers.major_page_faults_per_sec {
        if feature_window.major_page_faults_per_sec >= threshold {
            breaches.push(format!(
                "major_page_faults_per_sec:{}>={threshold}",
                feature_window.major_page_faults_per_sec
            ));
        }
    }

    if let Some(threshold) = policy.triggers.subprocess_start_delay_us {
        if feature_window.subprocess_start_delay_us_max >= threshold {
            breaches.push(format!(
                "subprocess_start_delay_us:{}>={threshold}",
                feature_window.subprocess_start_delay_us_max
            ));
        }
    }

    if let Some(threshold) = policy.triggers.queue_wait_us {
        if feature_window.queue_wait_us_max >= threshold {
            breaches.push(format!(
                "queue_wait_us:{}>={threshold}",
                feature_window.queue_wait_us_max
            ));
        }
    }

    if let Some(threshold) = policy.triggers.optional_io_latency_us {
        if feature_window.optional_io_latency_us_max >= threshold {
            breaches.push(format!(
                "optional_io_latency_us:{}>={threshold}",
                feature_window.optional_io_latency_us_max
            ));
        }
    }

    breaches
}

fn action_name(action: &Action) -> String {
    match action {
        Action::RaiseNice { delta } => format!("raise_nice:{delta}"),
        Action::SetAffinity {
            strategy,
            max_cpu_ratio,
        } => format!("set_affinity:{}:{max_cpu_ratio}", strategy.as_str()),
        Action::UseCpuset { enabled } => format!("use_cpuset:{enabled}"),
        Action::WarmupExecutor => "warmup_executor".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use super::PolicyEngine;
    use crate::{
        Action, EventContext, FeatureWindow, PinStrategy, PolicyContext, SafetyConfig,
        ScenarioActions, ScenarioKind, ScenarioPolicy, TriggerThresholds, WorkloadProfile,
        WorkloadTag,
    };

    #[test]
    fn clamps_actions_to_safety_limits() {
        let mut engine = PolicyEngine::new(
            BTreeMap::from([(
                ScenarioKind::InferenceTailGuard,
                ScenarioPolicy {
                    scenario: ScenarioKind::InferenceTailGuard,
                    enabled: true,
                    evaluation_window_ms: 500,
                    cooldown_ms: 0,
                    max_boost_duration_ms: 2_000,
                    triggers: TriggerThresholds {
                        run_queue_delay_us: Some(2_000),
                        ..TriggerThresholds::default()
                    },
                    actions: ScenarioActions {
                        raise_nice: Some(-9),
                        pin_strategy: Some(PinStrategy::PreferReservedCores),
                        ..ScenarioActions::default()
                    },
                },
            )]),
            SafetyConfig {
                require_revert: true,
                allow_background_throttle: false,
                max_priority_delta: 2,
                max_boost_duration_ms: 500,
                max_affinity_change_ratio: 1.5,
            },
        );

        let plan = engine
            .evaluate(&policy_context(
                ScenarioKind::InferenceTailGuard,
                [
                    WorkloadTag::AiInference,
                    WorkloadTag::InteractiveLatencySensitive,
                ],
                FeatureWindow {
                    pid: 42,
                    run_queue_delay_us_max: 2_500,
                    ..FeatureWindow::empty(42, 1_000)
                },
            ))
            .expect("plan should trigger");

        assert_eq!(plan.duration_ms, 500);
        assert!(plan.actions.contains(&Action::RaiseNice { delta: -2 }));
        assert!(plan.actions.iter().any(|action| {
            matches!(
                action,
                Action::SetAffinity {
                    strategy: PinStrategy::PreferReservedCores,
                    max_cpu_ratio
                } if (*max_cpu_ratio - 1.0).abs() < f32::EPSILON
            )
        }));
        assert_eq!(
            plan.audit_fields.get("priority_delta_clamped"),
            Some(&"-9->-2".to_string())
        );
    }

    #[test]
    fn enforces_cooldown_per_pid_and_scenario() {
        let mut engine = PolicyEngine::new(
            BTreeMap::from([(
                ScenarioKind::ToolCallBooster,
                ScenarioPolicy {
                    scenario: ScenarioKind::ToolCallBooster,
                    enabled: true,
                    evaluation_window_ms: 300,
                    cooldown_ms: 500,
                    max_boost_duration_ms: 500,
                    triggers: TriggerThresholds {
                        queue_wait_us: Some(2_000),
                        ..TriggerThresholds::default()
                    },
                    actions: ScenarioActions {
                        warmup_executor: Some(true),
                        ..ScenarioActions::default()
                    },
                },
            )]),
            default_safety(),
        );

        let first = policy_context(
            ScenarioKind::ToolCallBooster,
            [WorkloadTag::ToolCall],
            FeatureWindow {
                pid: 7,
                queue_wait_us_max: 2_500,
                ..FeatureWindow::empty(7, 1_000)
            },
        );
        let second = PolicyContext {
            event: EventContext::new(1_300, 7, "python"),
            ..first.clone()
        };
        let third = PolicyContext {
            event: EventContext::new(1_600, 7, "python"),
            ..first.clone()
        };

        assert!(engine.evaluate(&first).is_some());
        assert!(engine.evaluate(&second).is_none());
        assert!(engine.evaluate(&third).is_some());
    }

    #[test]
    fn resolves_conflicting_action_slots_by_scenario_priority() {
        let mut engine = PolicyEngine::new(
            BTreeMap::from([
                (
                    ScenarioKind::InferenceTailGuard,
                    ScenarioPolicy {
                        scenario: ScenarioKind::InferenceTailGuard,
                        enabled: true,
                        evaluation_window_ms: 500,
                        cooldown_ms: 0,
                        max_boost_duration_ms: 800,
                        triggers: TriggerThresholds {
                            run_queue_delay_us: Some(2_000),
                            ..TriggerThresholds::default()
                        },
                        actions: ScenarioActions {
                            raise_nice: Some(-5),
                            pin_strategy: Some(PinStrategy::PreferReservedCores),
                            ..ScenarioActions::default()
                        },
                    },
                ),
                (
                    ScenarioKind::ToolCallBooster,
                    ScenarioPolicy {
                        scenario: ScenarioKind::ToolCallBooster,
                        enabled: true,
                        evaluation_window_ms: 300,
                        cooldown_ms: 0,
                        max_boost_duration_ms: 800,
                        triggers: TriggerThresholds {
                            queue_wait_us: Some(2_000),
                            ..TriggerThresholds::default()
                        },
                        actions: ScenarioActions {
                            raise_nice: Some(-3),
                            pin_strategy: Some(PinStrategy::PreferLowContentionCores),
                            warmup_executor: Some(true),
                            ..ScenarioActions::default()
                        },
                    },
                ),
            ]),
            default_safety(),
        );

        let pid = 9;
        let inference_context = policy_context(
            ScenarioKind::InferenceTailGuard,
            [
                WorkloadTag::AiInference,
                WorkloadTag::InteractiveLatencySensitive,
                WorkloadTag::ToolCall,
            ],
            FeatureWindow {
                pid,
                run_queue_delay_us_max: 2_200,
                ..FeatureWindow::empty(pid, 5_000)
            },
        );
        let tool_context = policy_context(
            ScenarioKind::ToolCallBooster,
            [
                WorkloadTag::AiInference,
                WorkloadTag::InteractiveLatencySensitive,
                WorkloadTag::ToolCall,
            ],
            FeatureWindow {
                pid,
                queue_wait_us_max: 2_300,
                ..FeatureWindow::empty(pid, 5_000)
            },
        );
        let plans = engine.evaluate_all([&inference_context, &tool_context]);

        assert_eq!(plans.len(), 2);
        let inference = plans
            .iter()
            .find(|plan| plan.scenario == ScenarioKind::InferenceTailGuard)
            .expect("inference plan");
        let tool = plans
            .iter()
            .find(|plan| plan.scenario == ScenarioKind::ToolCallBooster)
            .expect("tool plan");

        assert!(inference
            .actions
            .iter()
            .any(|action| matches!(action, Action::RaiseNice { delta } if *delta == -5)));
        assert!(tool.actions.contains(&Action::WarmupExecutor));
        assert_eq!(tool.actions.len(), 1);
        assert_eq!(
            tool.audit_fields.get("suppressed_actions"),
            Some(
                &"raise_nice:-3=>inference_tail_guard,set_affinity:prefer_low_contention_cores:0.5=>inference_tail_guard"
                    .to_string()
            )
        );
    }

    #[test]
    fn skips_non_matching_profiles_and_empty_breaches() {
        let mut engine = PolicyEngine::new(
            BTreeMap::from([(
                ScenarioKind::InferenceTailGuard,
                ScenarioPolicy {
                    scenario: ScenarioKind::InferenceTailGuard,
                    enabled: true,
                    evaluation_window_ms: 500,
                    cooldown_ms: 0,
                    max_boost_duration_ms: 800,
                    triggers: TriggerThresholds {
                        run_queue_delay_us: Some(2_000),
                        ..TriggerThresholds::default()
                    },
                    actions: ScenarioActions {
                        raise_nice: Some(-3),
                        ..ScenarioActions::default()
                    },
                },
            )]),
            default_safety(),
        );

        let no_tag_match = policy_context(
            ScenarioKind::InferenceTailGuard,
            [WorkloadTag::ToolCall],
            FeatureWindow {
                pid: 1,
                run_queue_delay_us_max: 5_000,
                ..FeatureWindow::empty(1, 1_000)
            },
        );
        let no_breach = policy_context(
            ScenarioKind::InferenceTailGuard,
            [
                WorkloadTag::AiInference,
                WorkloadTag::InteractiveLatencySensitive,
            ],
            FeatureWindow::empty(1, 1_000),
        );

        assert!(engine.evaluate(&no_tag_match).is_none());
        assert!(engine.evaluate(&no_breach).is_none());
    }

    fn default_safety() -> SafetyConfig {
        SafetyConfig {
            require_revert: true,
            allow_background_throttle: false,
            max_priority_delta: 5,
            max_boost_duration_ms: 800,
            max_affinity_change_ratio: 0.5,
        }
    }

    fn policy_context<const N: usize>(
        scenario: ScenarioKind,
        tags: [WorkloadTag; N],
        feature_window: FeatureWindow,
    ) -> PolicyContext {
        let pid = feature_window.pid;
        let ended_at_ms = feature_window.ended_at_ms.max(1_000);

        PolicyContext {
            scenario,
            event: EventContext::new(ended_at_ms, pid, "python"),
            feature_window: FeatureWindow {
                ended_at_ms,
                ..feature_window
            },
            profile: WorkloadProfile::from_tags(
                pid,
                None,
                "python",
                tags.into_iter().collect::<BTreeSet<_>>(),
                vec!["test_rule".to_string()],
            ),
            audit_fields: BTreeMap::new(),
        }
    }
}
