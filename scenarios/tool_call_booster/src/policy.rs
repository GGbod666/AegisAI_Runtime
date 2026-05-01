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
    if context.scenario != ScenarioKind::ToolCallBooster {
        return None;
    }

    let stage = ToolCallStage::from_profile(&context.profile)?;
    let breaches = trigger_breaches(policy, &context.feature_window, stage);
    if breaches.is_empty() {
        return None;
    }

    let mut audit_fields = context.audit_fields.clone();
    let actions = build_actions(policy, safety, &mut audit_fields, stage);
    if actions.is_empty() {
        return None;
    }

    let duration_ms = stage_duration_ms(stage, policy, safety);
    if duration_ms == 0 {
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
    audit_fields.insert("tool_call_stage".to_string(), stage.as_str().to_string());
    audit_fields.insert("tool_call_focus".to_string(), stage.focus().to_string());
    audit_fields.insert(
        "tool_call_subchain".to_string(),
        stage.subchain().to_string(),
    );
    audit_fields.insert(
        "isolation_mode".to_string(),
        isolation_mode(stage, safety).to_string(),
    );
    audit_fields.insert(
        "isolation_scope".to_string(),
        stage.isolation_scope().to_string(),
    );
    audit_fields.insert(
        "background_isolation".to_string(),
        background_isolation_label(safety).to_string(),
    );
    if let Some(tool_call_id) = tool_call_id(&context.audit_fields, &context.profile) {
        audit_fields.insert("tool_call_id".to_string(), tool_call_id);
    }
    if duration_ms
        != policy
            .max_boost_duration_ms
            .min(safety.max_boost_duration_ms)
    {
        audit_fields.insert(
            "duration_scaled_by_stage".to_string(),
            format!("{}ms", duration_ms),
        );
    }

    Some(ActionPlan {
        scenario: context.scenario.clone(),
        target_pid: context.event.pid,
        target_process_name: context.event.process_name.clone(),
        actions,
        duration_ms,
        rationale: breaches,
        audit_fields,
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ToolCallStage {
    Executor,
    Retrieval,
    Rerank,
}

impl ToolCallStage {
    fn from_profile(profile: &WorkloadProfile) -> Option<Self> {
        if !profile.has_tag(&WorkloadTag::ToolCall) {
            return None;
        }

        if profile.has_tag(&WorkloadTag::RerankStage) {
            Some(Self::Rerank)
        } else if profile.has_tag(&WorkloadTag::RetrievalStage) {
            Some(Self::Retrieval)
        } else {
            Some(Self::Executor)
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Executor => "executor",
            Self::Retrieval => "retrieval",
            Self::Rerank => "rerank",
        }
    }

    fn focus(self) -> &'static str {
        match self {
            Self::Executor => "startup",
            Self::Retrieval => "fetch",
            Self::Rerank => "ranking",
        }
    }

    fn subchain(self) -> &'static str {
        match self {
            Self::Executor => "executor_startup",
            Self::Retrieval => "retrieval_io",
            Self::Rerank => "rerank_queue",
        }
    }

    fn isolation_scope(self) -> &'static str {
        match self {
            Self::Executor => "executor_process",
            Self::Retrieval => "retrieval_worker",
            Self::Rerank => "rerank_worker",
        }
    }

    fn duration_ratio(self) -> (u64, u64) {
        match self {
            Self::Executor => (1, 1),
            Self::Retrieval => (3, 4),
            Self::Rerank => (1, 2),
        }
    }

    fn supports_startup_delay(self) -> bool {
        matches!(self, Self::Executor)
    }

    fn supports_optional_io(self) -> bool {
        matches!(self, Self::Retrieval | Self::Rerank)
    }

    fn keeps_executor_warm(self) -> bool {
        matches!(self, Self::Executor | Self::Retrieval)
    }
}

fn isolation_mode(stage: ToolCallStage, safety: &SafetyConfig) -> &'static str {
    match (stage, safety.allow_background_throttle) {
        (ToolCallStage::Executor, true) => "executor_priority_with_background_soft_throttle",
        (ToolCallStage::Retrieval, true) => "retrieval_affinity_with_background_soft_throttle",
        (ToolCallStage::Rerank, true) => "rerank_affinity_with_background_soft_throttle",
        (ToolCallStage::Executor, false) => "executor_priority_only",
        (ToolCallStage::Retrieval, false) => "retrieval_affinity_only",
        (ToolCallStage::Rerank, false) => "rerank_affinity_only",
    }
}

fn background_isolation_label(safety: &SafetyConfig) -> &'static str {
    if safety.allow_background_throttle {
        "eligible"
    } else {
        "blocked_by_safety"
    }
}

fn tool_call_id(
    audit_fields: &BTreeMap<String, String>,
    profile: &WorkloadProfile,
) -> Option<String> {
    audit_fields.get("tool_call_id").cloned().or_else(|| {
        profile
            .matched_rules
            .iter()
            .find_map(|item| item.strip_prefix("tool_call_id=").map(str::to_string))
    })
}

fn trigger_breaches(
    policy: &ScenarioPolicy,
    feature_window: &FeatureWindow,
    stage: ToolCallStage,
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

    if stage.supports_startup_delay() {
        if let Some(threshold) = policy.triggers.subprocess_start_delay_us {
            if feature_window.subprocess_start_delay_us_max >= threshold {
                breaches.push(format!(
                    "subprocess_start_delay_us:{}>={threshold}",
                    feature_window.subprocess_start_delay_us_max
                ));
            }
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

    if stage.supports_optional_io() {
        if let Some(threshold) = policy.triggers.optional_io_latency_us {
            if feature_window.optional_io_latency_us_max >= threshold {
                breaches.push(format!(
                    "optional_io_latency_us:{}>={threshold}",
                    feature_window.optional_io_latency_us_max
                ));
            }
        }
    }

    breaches
}

fn build_actions(
    policy: &ScenarioPolicy,
    safety: &SafetyConfig,
    audit_fields: &mut BTreeMap<String, String>,
    stage: ToolCallStage,
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
        if stage.keeps_executor_warm() {
            actions.push(Action::WarmupExecutor);
        } else {
            audit_fields.insert(
                "warmup_executor_skipped".to_string(),
                "rerank_stage".to_string(),
            );
        }
    }

    actions
}

fn stage_duration_ms(stage: ToolCallStage, policy: &ScenarioPolicy, safety: &SafetyConfig) -> u64 {
    let base_duration_ms = policy
        .max_boost_duration_ms
        .min(safety.max_boost_duration_ms);
    if base_duration_ms == 0 {
        return 0;
    }

    let (numerator, denominator) = stage.duration_ratio();
    let scaled_duration_ms = base_duration_ms
        .saturating_mul(numerator)
        .checked_div(denominator)
        .unwrap_or(base_duration_ms);

    scaled_duration_ms.max(1)
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
    fn classifies_tool_call_stage_and_scales_duration() {
        let policy = policy();
        let safety = default_safety();

        let executor = evaluate(
            &policy,
            &safety,
            &context(
                [WorkloadTag::ToolCall],
                FeatureWindow {
                    pid: 42,
                    subprocess_start_delay_us_max: 1_700,
                    ended_at_ms: 1_000,
                    ..FeatureWindow::empty(42, 1_000)
                },
            ),
        )
        .expect("executor stage should trigger");
        assert_eq!(executor.duration_ms, 800);
        assert_eq!(
            executor.audit_fields.get("tool_call_stage"),
            Some(&"executor".to_string())
        );
        assert_eq!(
            executor.audit_fields.get("tool_call_focus"),
            Some(&"startup".to_string())
        );
        assert_eq!(
            executor.audit_fields.get("tool_call_subchain"),
            Some(&"executor_startup".to_string())
        );
        assert_eq!(
            executor.audit_fields.get("isolation_mode"),
            Some(&"executor_priority_only".to_string())
        );
        assert_eq!(
            executor.audit_fields.get("background_isolation"),
            Some(&"blocked_by_safety".to_string())
        );

        let retrieval = evaluate(
            &policy,
            &safety,
            &context(
                [WorkloadTag::ToolCall, WorkloadTag::RetrievalStage],
                FeatureWindow {
                    pid: 42,
                    queue_wait_us_max: 2_500,
                    ended_at_ms: 1_000,
                    ..FeatureWindow::empty(42, 1_000)
                },
            ),
        )
        .expect("retrieval stage should trigger");
        assert_eq!(retrieval.duration_ms, 600);
        assert_eq!(
            retrieval.audit_fields.get("tool_call_stage"),
            Some(&"retrieval".to_string())
        );
        assert_eq!(
            retrieval.audit_fields.get("tool_call_subchain"),
            Some(&"retrieval_io".to_string())
        );
        assert_eq!(
            retrieval.audit_fields.get("isolation_scope"),
            Some(&"retrieval_worker".to_string())
        );
        assert!(retrieval.actions.contains(&Action::WarmupExecutor));

        let rerank = evaluate(
            &policy,
            &safety,
            &context(
                [WorkloadTag::ToolCall, WorkloadTag::RerankStage],
                FeatureWindow {
                    pid: 42,
                    queue_wait_us_max: 2_500,
                    ended_at_ms: 1_000,
                    ..FeatureWindow::empty(42, 1_000)
                },
            ),
        )
        .expect("rerank stage should trigger");
        assert_eq!(rerank.duration_ms, 400);
        assert_eq!(
            rerank.audit_fields.get("tool_call_stage"),
            Some(&"rerank".to_string())
        );
        assert_eq!(
            rerank.audit_fields.get("isolation_mode"),
            Some(&"rerank_affinity_only".to_string())
        );
        assert!(!rerank.actions.contains(&Action::WarmupExecutor));
        assert_eq!(
            rerank.audit_fields.get("warmup_executor_skipped"),
            Some(&"rerank_stage".to_string())
        );
    }

    #[test]
    fn startup_delay_only_triggers_executor_and_io_focuses_workers() {
        let mut policy = policy();
        policy.triggers = TriggerThresholds {
            subprocess_start_delay_us: Some(1_500),
            optional_io_latency_us: Some(4_000),
            ..TriggerThresholds::default()
        };

        let executor = evaluate(
            &policy,
            &default_safety(),
            &context(
                [WorkloadTag::ToolCall],
                FeatureWindow {
                    pid: 7,
                    subprocess_start_delay_us_max: 1_800,
                    ended_at_ms: 1_000,
                    ..FeatureWindow::empty(7, 1_000)
                },
            ),
        );
        assert!(executor.is_some());

        let retrieval = evaluate(
            &policy,
            &default_safety(),
            &context(
                [WorkloadTag::ToolCall, WorkloadTag::RetrievalStage],
                FeatureWindow {
                    pid: 7,
                    subprocess_start_delay_us_max: 1_800,
                    ended_at_ms: 1_000,
                    ..FeatureWindow::empty(7, 1_000)
                },
            ),
        );
        assert!(retrieval.is_none());

        let rerank = evaluate(
            &policy,
            &default_safety(),
            &context(
                [WorkloadTag::ToolCall, WorkloadTag::RerankStage],
                FeatureWindow {
                    pid: 7,
                    optional_io_latency_us_max: 4_500,
                    ended_at_ms: 1_000,
                    ..FeatureWindow::empty(7, 1_000)
                },
            ),
        )
        .expect("rerank io latency should trigger");

        assert!(rerank
            .rationale
            .contains(&"optional_io_latency_us:4500>=4000".to_string()));
    }

    #[test]
    fn clamps_actions_to_safety_limits() {
        let mut policy = policy();
        policy.actions.raise_nice = Some(-9);
        policy.actions.pin_strategy = Some(PinStrategy::PreferReservedCores);

        let plan = evaluate(
            &policy,
            &SafetyConfig {
                require_revert: true,
                allow_background_throttle: false,
                max_priority_delta: 2,
                max_boost_duration_ms: 800,
                max_affinity_change_ratio: 1.4,
            },
            &context(
                [WorkloadTag::ToolCall, WorkloadTag::RetrievalStage],
                FeatureWindow {
                    pid: 42,
                    queue_wait_us_max: 2_500,
                    ended_at_ms: 1_000,
                    ..FeatureWindow::empty(42, 1_000)
                },
            ),
        )
        .expect("retrieval stage should trigger");

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
        assert_eq!(
            plan.audit_fields.get("duration_scaled_by_stage"),
            Some(&"600ms".to_string())
        );
    }

    #[test]
    fn carries_tool_call_id_and_background_isolation_eligibility() {
        let mut context = context(
            [WorkloadTag::ToolCall, WorkloadTag::RetrievalStage],
            FeatureWindow {
                pid: 42,
                queue_wait_us_max: 2_500,
                ended_at_ms: 1_000,
                ..FeatureWindow::empty(42, 1_000)
            },
        );
        context
            .audit_fields
            .insert("tool_call_id".to_string(), "tc-001".to_string());

        let plan = evaluate(
            &policy(),
            &SafetyConfig {
                allow_background_throttle: true,
                ..default_safety()
            },
            &context,
        )
        .expect("retrieval stage should trigger");

        assert_eq!(
            plan.audit_fields.get("tool_call_id"),
            Some(&"tc-001".to_string())
        );
        assert_eq!(
            plan.audit_fields.get("background_isolation"),
            Some(&"eligible".to_string())
        );
        assert_eq!(
            plan.audit_fields.get("isolation_mode"),
            Some(&"retrieval_affinity_with_background_soft_throttle".to_string())
        );
    }

    fn policy() -> ScenarioPolicy {
        ScenarioPolicy {
            scenario: ScenarioKind::ToolCallBooster,
            enabled: true,
            evaluation_window_ms: 300,
            cooldown_ms: 800,
            max_boost_duration_ms: 1_200,
            triggers: TriggerThresholds {
                subprocess_start_delay_us: Some(1_500),
                queue_wait_us: Some(2_000),
                optional_io_latency_us: Some(4_000),
                ..TriggerThresholds::default()
            },
            actions: ScenarioActions {
                raise_nice: Some(-3),
                pin_strategy: Some(PinStrategy::PreferLowContentionCores),
                use_cpuset: None,
                warmup_executor: Some(true),
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

    fn context<const N: usize>(
        tags: [WorkloadTag; N],
        feature_window: FeatureWindow,
    ) -> PolicyContext {
        let pid = feature_window.pid;
        PolicyContext {
            scenario: ScenarioKind::ToolCallBooster,
            event: EventContext::new(1_000, pid, "python"),
            feature_window,
            profile: WorkloadProfile::from_tags(
                pid,
                None,
                "python",
                tags.into_iter().collect::<BTreeSet<_>>(),
                vec!["tool-executor".to_string()],
            ),
            audit_fields: BTreeMap::new(),
        }
    }
}
