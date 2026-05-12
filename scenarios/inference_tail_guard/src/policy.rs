use std::collections::BTreeMap;

use crate::config::{SafetyConfig, ScenarioPolicy};
use crate::model::{
    Action, ActionPlan, FeatureWindow, PolicyContext, ScenarioKind, WorkloadProfile, WorkloadTag,
};

pub(crate) fn evaluate(
    policy: &ScenarioPolicy,
    safety: &SafetyConfig,
    context: &PolicyContext,
) -> Option<ActionPlan> {
    if context.scenario != ScenarioKind::InferenceTailGuard || !matches_profile(&context.profile) {
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

fn matches_profile(profile: &WorkloadProfile) -> bool {
    profile.has_tag(&WorkloadTag::AiInference)
        && profile.has_tag(&WorkloadTag::InteractiveLatencySensitive)
}

fn trigger_breaches(policy: &ScenarioPolicy, feature_window: &FeatureWindow) -> Vec<String> {
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

fn build_actions(
    policy: &ScenarioPolicy,
    safety: &SafetyConfig,
    audit_fields: &mut BTreeMap<String, String>,
) -> Vec<Action> {
    let mut actions = Vec::new();

    if let Some(delta) = policy.actions.raise_nice {
        let bounded_delta = clamp_priority_delta(delta, safety.normalized_max_priority_delta());
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
        } else if delta != 0 {
            audit_fields.insert("priority_delta_clamped".to_string(), format!("{delta}->0"));
        }
    }

    if let Some(strategy) = &policy.actions.pin_strategy {
        let requested_cpu_ratio = safety.max_affinity_change_ratio;
        let max_cpu_ratio = safety.normalized_max_affinity_change_ratio();
        if !requested_cpu_ratio.is_finite()
            || (max_cpu_ratio - requested_cpu_ratio).abs() > f32::EPSILON
        {
            audit_fields.insert(
                "affinity_ratio_clamped".to_string(),
                format!("{requested_cpu_ratio}->{max_cpu_ratio}"),
            );
        }

        if max_cpu_ratio > 0.0 {
            actions.push(Action::SetAffinity {
                strategy: strategy.clone(),
                max_cpu_ratio,
            });
        } else {
            audit_fields.insert(
                "affinity_action_skipped".to_string(),
                "max_cpu_ratio_zero".to_string(),
            );
        }
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

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use super::evaluate;
    use crate::{
        Action, EventContext, FeatureWindow, PinStrategy, PolicyContext, SafetyConfig,
        ScenarioActions, ScenarioKind, ScenarioPolicy, TriggerThresholds, WorkloadProfile,
        WorkloadTag,
    };

    #[test]
    fn only_matches_interactive_ai_inference_profiles() {
        let policy = policy();
        let safety = default_safety();

        assert!(evaluate(&policy, &safety, &context([WorkloadTag::AiInference])).is_none());
        assert!(evaluate(
            &policy,
            &safety,
            &context([WorkloadTag::InteractiveLatencySensitive]),
        )
        .is_none());

        let plan = evaluate(
            &policy,
            &safety,
            &context([
                WorkloadTag::AiInference,
                WorkloadTag::InteractiveLatencySensitive,
            ]),
        )
        .expect("interactive inference profile should trigger");

        assert_eq!(plan.scenario, ScenarioKind::InferenceTailGuard);
        assert_eq!(
            plan.rationale,
            vec!["run_queue_delay_us:2500>=2000".to_string()]
        );
    }

    #[test]
    fn clamps_actions_and_supports_tail_signals() {
        let mut policy = policy();
        policy.triggers = TriggerThresholds {
            offcpu_spike_us: Some(3_000),
            major_page_faults_per_sec: Some(3),
            optional_io_latency_us: Some(4_000),
            ..TriggerThresholds::default()
        };
        policy.actions.raise_nice = Some(-9);
        policy.actions.pin_strategy = Some(PinStrategy::PreferReservedCores);

        let plan = evaluate(
            &policy,
            &SafetyConfig {
                require_revert: true,
                allow_background_throttle: false,
                max_priority_delta: 2,
                max_boost_duration_ms: 500,
                max_affinity_change_ratio: 1.4,
            },
            &context_with_window(
                [
                    WorkloadTag::AiInference,
                    WorkloadTag::InteractiveLatencySensitive,
                ],
                FeatureWindow {
                    pid: 42,
                    offcpu_time_us_max: 3_500,
                    major_page_faults_per_sec: 4,
                    optional_io_latency_us_max: 4_500,
                    ..FeatureWindow::empty(42, 1_000)
                },
            ),
        )
        .expect("tail signals should trigger");

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
        assert!(plan
            .rationale
            .contains(&"offcpu_spike_us:3500>=3000".to_string()));
        assert!(plan
            .rationale
            .contains(&"major_page_faults_per_sec:4>=3".to_string()));
        assert!(plan
            .rationale
            .contains(&"optional_io_latency_us:4500>=4000".to_string()));
    }

    fn policy() -> ScenarioPolicy {
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
                use_cpuset: Some(false),
                warmup_executor: None,
            },
        }
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

    fn context<const N: usize>(tags: [WorkloadTag; N]) -> PolicyContext {
        context_with_window(
            tags,
            FeatureWindow {
                pid: 42,
                run_queue_delay_us_max: 2_500,
                ended_at_ms: 1_000,
                ..FeatureWindow::empty(42, 1_000)
            },
        )
    }

    fn context_with_window<const N: usize>(
        tags: [WorkloadTag; N],
        feature_window: FeatureWindow,
    ) -> PolicyContext {
        let pid = 42;
        PolicyContext {
            scenario: ScenarioKind::InferenceTailGuard,
            event: EventContext::new(1_000, pid, "ollama"),
            feature_window,
            profile: WorkloadProfile::from_tags(
                pid,
                None,
                "ollama",
                tags.into_iter().collect::<BTreeSet<_>>(),
                vec!["runtime.process_names:ollama".to_string()],
            ),
            audit_fields: BTreeMap::new(),
        }
    }
}
