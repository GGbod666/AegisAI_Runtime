use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClassifierOptions {
    pub enable_cmdline_rules: bool,
    pub enable_cgroup_rules: bool,
    pub enable_parent_child_inference: bool,
    pub enable_pid_allowlist: bool,
}

impl Default for ClassifierOptions {
    fn default() -> Self {
        Self {
            enable_cmdline_rules: true,
            enable_cgroup_rules: true,
            enable_parent_child_inference: true,
            enable_pid_allowlist: true,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProcessSnapshot {
    pub pid: u32,
    pub tid: Option<u32>,
    pub process_name: String,
    pub cmdline: String,
    pub cgroup_path: Option<String>,
    pub tags: BTreeSet<String>,
    pub parent: Option<ProcessRef>,
}

impl ProcessSnapshot {
    pub fn new(pid: u32, process_name: impl Into<String>, cmdline: impl Into<String>) -> Self {
        Self {
            pid,
            tid: None,
            process_name: process_name.into(),
            cmdline: cmdline.into(),
            cgroup_path: None,
            tags: BTreeSet::new(),
            parent: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProcessRef {
    pub pid: u32,
    pub process_name: String,
    pub cmdline: String,
    pub cgroup_path: Option<String>,
    pub tags: BTreeSet<WorkloadTag>,
}

impl ProcessRef {
    pub fn new(pid: u32, process_name: impl Into<String>, cmdline: impl Into<String>) -> Self {
        Self {
            pid,
            process_name: process_name.into(),
            cmdline: cmdline.into(),
            cgroup_path: None,
            tags: BTreeSet::new(),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProcessRule {
    pub id: String,
    pub process_name: Option<String>,
    pub cmdline_contains: Option<String>,
    pub cgroup_contains: Option<String>,
    pub pid_allowlist: BTreeSet<u32>,
    pub tag_markers: BTreeSet<String>,
    pub parent_process_name: Option<String>,
    pub parent_cmdline_contains: Option<String>,
    pub parent_has_any_tags: BTreeSet<WorkloadTag>,
    pub tags: BTreeSet<WorkloadTag>,
}

impl ProcessRule {
    pub fn has_matchers(&self) -> bool {
        self.process_name.is_some()
            || self.cmdline_contains.is_some()
            || self.cgroup_contains.is_some()
            || !self.pid_allowlist.is_empty()
            || !self.tag_markers.is_empty()
            || self.parent_process_name.is_some()
            || self.parent_cmdline_contains.is_some()
            || !self.parent_has_any_tags.is_empty()
    }

    pub fn matches(&self, snapshot: &ProcessSnapshot) -> bool {
        self.matches_with_options(snapshot, &ClassifierOptions::default())
    }

    pub fn matches_with_options(
        &self,
        snapshot: &ProcessSnapshot,
        options: &ClassifierOptions,
    ) -> bool {
        let mut evaluated_matcher = false;

        if let Some(process_name) = &self.process_name {
            evaluated_matcher = true;
            if !eq_ignore_ascii_case(&snapshot.process_name, process_name) {
                return false;
            }
        }

        if options.enable_cmdline_rules {
            if let Some(cmdline_contains) = &self.cmdline_contains {
                evaluated_matcher = true;
                if !contains_ignore_ascii_case(&snapshot.cmdline, cmdline_contains) {
                    return false;
                }
            }
        }

        if options.enable_cgroup_rules {
            if let Some(cgroup_contains) = &self.cgroup_contains {
                evaluated_matcher = true;
                let Some(cgroup_path) = snapshot.cgroup_path.as_deref() else {
                    return false;
                };

                if !contains_ignore_ascii_case(cgroup_path, cgroup_contains) {
                    return false;
                }
            }
        }

        if options.enable_pid_allowlist && !self.pid_allowlist.is_empty() {
            evaluated_matcher = true;
            if !self.pid_allowlist.contains(&snapshot.pid) {
                return false;
            }
        }

        if !self.tag_markers.is_empty() {
            evaluated_matcher = true;
            if !self.tag_markers.iter().all(|marker| {
                snapshot
                    .tags
                    .iter()
                    .any(|tag| eq_ignore_ascii_case(tag, marker))
            }) {
                return false;
            }
        }

        if options.enable_parent_child_inference
            && (self.parent_process_name.is_some()
                || self.parent_cmdline_contains.is_some()
                || !self.parent_has_any_tags.is_empty())
        {
            evaluated_matcher = true;
            let Some(parent) = snapshot.parent.as_ref() else {
                return false;
            };

            if let Some(parent_process_name) = &self.parent_process_name {
                if !eq_ignore_ascii_case(&parent.process_name, parent_process_name) {
                    return false;
                }
            }

            if let Some(parent_cmdline_contains) = &self.parent_cmdline_contains {
                if !contains_ignore_ascii_case(&parent.cmdline, parent_cmdline_contains) {
                    return false;
                }
            }

            if !self.parent_has_any_tags.is_empty()
                && parent
                    .tags
                    .intersection(&self.parent_has_any_tags)
                    .next()
                    .is_none()
            {
                return false;
            }
        }

        evaluated_matcher
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkloadProfile {
    pub pid: u32,
    pub tid: Option<u32>,
    pub scope: OwnershipScope,
    pub workload_class: WorkloadClass,
    pub stage: StageLabel,
    pub latency_sensitivity: LatencySensitivity,
    pub tags: BTreeSet<WorkloadTag>,
    pub matched_rules: Vec<String>,
}

impl WorkloadProfile {
    pub fn from_process(
        snapshot: &ProcessSnapshot,
        tags: BTreeSet<WorkloadTag>,
        matched_rules: Vec<String>,
    ) -> Self {
        Self {
            pid: snapshot.pid,
            tid: snapshot.tid,
            scope: OwnershipScope::Process,
            workload_class: WorkloadClass::from_tags(&tags),
            stage: StageLabel::from_tags(&tags),
            latency_sensitivity: LatencySensitivity::from_tags(&tags),
            tags,
            matched_rules,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OwnershipScope {
    Process,
    Thread,
    Cgroup,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkloadClass {
    Unknown,
    AiInference,
    ToolCall,
    BackgroundJob,
}

impl WorkloadClass {
    pub fn from_tags(tags: &BTreeSet<WorkloadTag>) -> Self {
        if tags.contains(&WorkloadTag::ToolCall)
            || tags.contains(&WorkloadTag::RetrievalStage)
            || tags.contains(&WorkloadTag::RerankStage)
        {
            Self::ToolCall
        } else if tags.contains(&WorkloadTag::AiInference) {
            Self::AiInference
        } else if tags.contains(&WorkloadTag::BackgroundJob) {
            Self::BackgroundJob
        } else {
            Self::Unknown
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StageLabel {
    Unknown,
    Inference,
    ToolCall,
    Retrieval,
    Rerank,
    Background,
}

impl StageLabel {
    pub fn from_tags(tags: &BTreeSet<WorkloadTag>) -> Self {
        if tags.contains(&WorkloadTag::RerankStage) {
            Self::Rerank
        } else if tags.contains(&WorkloadTag::RetrievalStage) {
            Self::Retrieval
        } else if tags.contains(&WorkloadTag::ToolCall) {
            Self::ToolCall
        } else if tags.contains(&WorkloadTag::AiInference) {
            Self::Inference
        } else if tags.contains(&WorkloadTag::BackgroundJob) {
            Self::Background
        } else {
            Self::Unknown
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LatencySensitivity {
    Unknown,
    Interactive,
    Batch,
}

impl LatencySensitivity {
    pub fn from_tags(tags: &BTreeSet<WorkloadTag>) -> Self {
        if tags.contains(&WorkloadTag::InteractiveLatencySensitive) {
            Self::Interactive
        } else if tags.contains(&WorkloadTag::BackgroundJob) {
            Self::Batch
        } else {
            Self::Unknown
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum WorkloadTag {
    AiInference,
    ToolCall,
    RetrievalStage,
    RerankStage,
    BackgroundJob,
    InteractiveLatencySensitive,
    Custom(String),
}

impl WorkloadTag {
    pub fn parse(value: &str) -> Self {
        let raw = value.trim();
        let normalized = raw.to_ascii_uppercase();

        match normalized.as_str() {
            "AI_INFERENCE" => Self::AiInference,
            "TOOL_CALL" => Self::ToolCall,
            "RETRIEVAL_STAGE" => Self::RetrievalStage,
            "RERANK_STAGE" => Self::RerankStage,
            "BACKGROUND_JOB" => Self::BackgroundJob,
            "INTERACTIVE_LATENCY_SENSITIVE" => Self::InteractiveLatencySensitive,
            _ => Self::Custom(raw.to_string()),
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            Self::AiInference => "AI_INFERENCE",
            Self::ToolCall => "TOOL_CALL",
            Self::RetrievalStage => "RETRIEVAL_STAGE",
            Self::RerankStage => "RERANK_STAGE",
            Self::BackgroundJob => "BACKGROUND_JOB",
            Self::InteractiveLatencySensitive => "INTERACTIVE_LATENCY_SENSITIVE",
            Self::Custom(value) => value.as_str(),
        }
    }
}

impl From<&str> for WorkloadTag {
    fn from(value: &str) -> Self {
        Self::parse(value)
    }
}

fn eq_ignore_ascii_case(left: &str, right: &str) -> bool {
    left.eq_ignore_ascii_case(right)
}

fn contains_ignore_ascii_case(haystack: &str, needle: &str) -> bool {
    haystack
        .to_ascii_lowercase()
        .contains(&needle.to_ascii_lowercase())
}
