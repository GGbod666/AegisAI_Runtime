#![forbid(unsafe_code)]

//! Rule-based AI workload classifier for the AegisAI Runtime control loop.
//!
//! This crate turns process-level runtime metadata into a stable semantic
//! workload profile that downstream policy modules can consume.

mod classifier;
mod config;
mod model;

pub use classifier::Classifier;
pub use config::{ClassifierConfig, ConfigError};
pub use model::{
    ClassifierOptions, LatencySensitivity, OwnershipScope, ProcessRef, ProcessRule,
    ProcessSnapshot, StageLabel, WorkloadClass, WorkloadProfile, WorkloadTag,
};

#[cfg(test)]
mod tests {
    use crate::{
        Classifier, ClassifierConfig, ClassifierOptions, LatencySensitivity, ProcessRef,
        ProcessSnapshot, StageLabel, WorkloadClass, WorkloadTag,
    };

    #[test]
    fn parses_example_classifier_config() {
        let config = ClassifierConfig::from_toml_str(include_str!(
            "../../../configs/classifier/process_rules.example.toml"
        ))
        .expect("example config should parse");

        assert_eq!(config.process_rules.len(), 7);
        assert_eq!(
            config.process_rules[0].process_name.as_deref(),
            Some("ollama")
        );
        assert!(config.process_rules[0]
            .tags
            .contains(&WorkloadTag::AiInference));
    }

    #[test]
    fn classifies_inference_process_from_example_config() {
        let config = ClassifierConfig::from_toml_str(include_str!(
            "../../../configs/classifier/process_rules.example.toml"
        ))
        .expect("example config should parse");
        let classifier = Classifier::from_config(config);
        let snapshot = ProcessSnapshot::new(42, "ollama", "/usr/bin/ollama serve");

        let profile = classifier.classify_process(&snapshot);

        assert_eq!(profile.workload_class, WorkloadClass::AiInference);
        assert_eq!(profile.stage, StageLabel::Inference);
        assert_eq!(profile.latency_sensitivity, LatencySensitivity::Interactive);
        assert!(profile.tags.contains(&WorkloadTag::AiInference));
        assert!(profile
            .tags
            .contains(&WorkloadTag::InteractiveLatencySensitive));
        assert_eq!(profile.matched_rules.len(), 1);
    }

    #[test]
    fn classifies_retrieval_stage_from_cmdline() {
        let config = ClassifierConfig::from_toml_str(include_str!(
            "../../../configs/classifier/process_rules.example.toml"
        ))
        .expect("example config should parse");
        let classifier = Classifier::from_config(config);
        let snapshot = ProcessSnapshot::new(
            1001,
            "python",
            "python worker.py --role retrieval-worker --request-id 1",
        );

        let profile = classifier.classify_process(&snapshot);

        assert_eq!(profile.workload_class, WorkloadClass::ToolCall);
        assert_eq!(profile.stage, StageLabel::Retrieval);
        assert!(profile.tags.contains(&WorkloadTag::ToolCall));
        assert!(profile.tags.contains(&WorkloadTag::RetrievalStage));
    }

    #[test]
    fn supports_parent_relationship_and_pid_allowlist_rules() {
        let config = ClassifierConfig::from_toml_str(
            r#"
            [[process_rules]]
            id = "interactive-parent"
            name = "sandbox-worker"
            parent_has_any_tags = ["TOOL_CALL"]
            tags = ["TOOL_CALL", "INTERACTIVE_LATENCY_SENSITIVE"]

            [[process_rules]]
            id = "pinned-batch-pid"
            pids = [2048]
            tags = ["BACKGROUND_JOB"]
            "#,
        )
        .expect("custom config should parse");
        let classifier = Classifier::from_config(config);

        let mut parent = ProcessRef::new(7, "tool-executor", "tool-executor --serve");
        parent.tags = std::iter::once(WorkloadTag::ToolCall).collect();

        let mut child = ProcessSnapshot::new(8, "sandbox-worker", "sandbox-worker --job");
        child.parent = Some(parent);

        let profile = classifier.classify_process(&child);
        assert_eq!(profile.workload_class, WorkloadClass::ToolCall);
        assert_eq!(profile.latency_sensitivity, LatencySensitivity::Interactive);

        let pinned_background = ProcessSnapshot::new(2048, "python", "python batch.py");
        let pinned_profile = classifier.classify_process(&pinned_background);
        assert_eq!(pinned_profile.workload_class, WorkloadClass::BackgroundJob);
        assert_eq!(pinned_profile.stage, StageLabel::Background);
    }

    #[test]
    fn supports_cgroup_and_tag_marker_rules() {
        let config = ClassifierConfig::from_toml_str(
            r#"
            [[process_rules]]
            id = "interactive-cgroup"
            cgroup_contains = "interactive.slice"
            tag_markers = ["frontend"]
            tags = ["AI_INFERENCE", "INTERACTIVE_LATENCY_SENSITIVE"]
            "#,
        )
        .expect("custom config should parse");
        let classifier = Classifier::from_config(config);

        let mut snapshot = ProcessSnapshot::new(512, "python", "python server.py");
        snapshot.cgroup_path = Some("/sys/fs/cgroup/interactive.slice/service".to_string());
        snapshot.tags = std::iter::once("frontend".to_string()).collect();

        let profile = classifier.classify_process(&snapshot);

        assert_eq!(profile.workload_class, WorkloadClass::AiInference);
        assert_eq!(profile.latency_sensitivity, LatencySensitivity::Interactive);
    }

    #[test]
    fn respects_disabled_matcher_options() {
        let config = ClassifierConfig::from_toml_str(
            r#"
            [[process_rules]]
            id = "cmdline-only"
            cmdline_contains = "tool-executor"
            tags = ["TOOL_CALL"]

            [[process_rules]]
            id = "parent-only"
            parent_name = "ollama"
            tags = ["AI_INFERENCE"]
            "#,
        )
        .expect("custom config should parse");
        let classifier = Classifier::with_options(
            config.process_rules,
            ClassifierOptions {
                enable_cmdline_rules: false,
                enable_parent_child_inference: false,
                ..ClassifierOptions::default()
            },
        );

        let mut snapshot =
            ProcessSnapshot::new(2048, "python", "python tool-executor --request-id 7");
        snapshot.parent = Some(ProcessRef::new(1, "ollama", "ollama serve"));

        let profile = classifier.classify_process(&snapshot);

        assert_eq!(profile.workload_class, WorkloadClass::Unknown);
        assert!(profile.tags.is_empty());
        assert!(profile.matched_rules.is_empty());
    }
}
