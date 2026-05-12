use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path};

const AEGISAI_CGROUP_ROOT: &str = "/sys/fs/cgroup/aegisai.runtime";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CpusetDryRunMode {
    DryRun,
    LiveWrite,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CpusetProcessClassification {
    InteractiveAiInference,
    ActiveToolCall,
    BackgroundJob,
    Unknown,
}

impl CpusetProcessClassification {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::InteractiveAiInference => "INTERACTIVE_AI_INFERENCE",
            Self::ActiveToolCall => "ACTIVE_TOOL_CALL",
            Self::BackgroundJob => "BACKGROUND_JOB",
            Self::Unknown => "UNKNOWN",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CpusetProcessTarget {
    pub pid: u32,
    pub process_name: String,
    pub cgroup: Option<String>,
    pub classification: Option<CpusetProcessClassification>,
}

impl CpusetProcessTarget {
    pub fn new(
        pid: u32,
        process_name: impl Into<String>,
        classification: Option<CpusetProcessClassification>,
    ) -> Self {
        Self {
            pid,
            process_name: process_name.into(),
            cgroup: None,
            classification,
        }
    }

    pub fn with_cgroup(mut self, cgroup: impl Into<String>) -> Self {
        self.cgroup = Some(cgroup.into());
        self
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CpusetRollbackCapture {
    pub original_memberships: BTreeMap<u32, String>,
    pub original_cpuset_cpus: BTreeMap<String, String>,
    pub original_cpuset_mems: BTreeMap<String, String>,
    pub original_cpu_max: BTreeMap<String, String>,
    pub temporary_cgroups: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CpusetDryRunRequest {
    pub mode: CpusetDryRunMode,
    pub cgroup_root: String,
    pub protected_cgroup: String,
    pub background_cgroup: String,
    pub proposed_cpus: Vec<u32>,
    pub cpuset_mems: Option<String>,
    pub protected_targets: Vec<CpusetProcessTarget>,
    pub background_targets: Vec<CpusetProcessTarget>,
    pub rollback_capture: Option<CpusetRollbackCapture>,
    pub max_processes: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CpusetDryRunTargetContext {
    pub cgroup_root: String,
    pub protected_cgroup: String,
    pub background_cgroup: String,
    pub protected_pids: Vec<u32>,
    pub background_pids: Vec<u32>,
    pub proposed_cpus: Vec<u32>,
    pub cpuset_mems: Option<String>,
    pub affected_process_count: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CpusetCapturePlan {
    pub required_membership_pids: Vec<u32>,
    pub required_cgroup_files: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CpusetRollbackPlan {
    pub restore_membership_pids: Vec<u32>,
    pub restore_cgroup_files: Vec<String>,
    pub remove_temporary_cgroups: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CpusetDryRunRejectionReason {
    UnsupportedLiveWriteMode,
    UnsafeCgroupRoot,
    MissingProcessClassification,
    EmptyComputedCpuSet,
    MissingRollbackCapture,
    OverbroadProcessSet,
}

impl CpusetDryRunRejectionReason {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::UnsupportedLiveWriteMode => "unsupported_live_write_mode",
            Self::UnsafeCgroupRoot => "unsafe_cgroup_root",
            Self::MissingProcessClassification => "missing_process_classification",
            Self::EmptyComputedCpuSet => "empty_computed_cpu_set",
            Self::MissingRollbackCapture => "missing_rollback_capture",
            Self::OverbroadProcessSet => "overbroad_process_set",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CpusetDryRunRejection {
    pub reason: CpusetDryRunRejectionReason,
    pub detail: String,
    pub target_context: CpusetDryRunTargetContext,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CpusetDryRunPlan {
    pub target_context: CpusetDryRunTargetContext,
    pub capture_plan: CpusetCapturePlan,
    pub rollback_plan: Option<CpusetRollbackPlan>,
    pub rejection: Option<CpusetDryRunRejection>,
}

impl CpusetDryRunPlan {
    pub fn accepted(&self) -> bool {
        self.rejection.is_none()
    }
}

pub fn plan_cpuset_dry_run(request: CpusetDryRunRequest) -> CpusetDryRunPlan {
    let target_context = CpusetDryRunTargetContext::from_request(&request);
    let capture_plan = CpusetCapturePlan::from_request(&request);
    let rollback_plan = request
        .rollback_capture
        .as_ref()
        .map(|capture| CpusetRollbackPlan::from_capture(capture, &request));

    let rejection =
        first_rejection(&request, &target_context).map(|(reason, detail)| CpusetDryRunRejection {
            reason,
            detail,
            target_context: target_context.clone(),
        });

    CpusetDryRunPlan {
        target_context,
        capture_plan,
        rollback_plan,
        rejection,
    }
}

impl CpusetDryRunTargetContext {
    fn from_request(request: &CpusetDryRunRequest) -> Self {
        Self {
            cgroup_root: request.cgroup_root.clone(),
            protected_cgroup: request.protected_cgroup.clone(),
            background_cgroup: request.background_cgroup.clone(),
            protected_pids: sorted_pids(&request.protected_targets),
            background_pids: sorted_pids(&request.background_targets),
            proposed_cpus: normalized_cpus(&request.proposed_cpus),
            cpuset_mems: request.cpuset_mems.clone(),
            affected_process_count: request.protected_targets.len()
                + request.background_targets.len(),
        }
    }
}

impl CpusetCapturePlan {
    fn from_request(request: &CpusetDryRunRequest) -> Self {
        let mut required_cgroup_files = BTreeSet::new();
        for cgroup in [&request.protected_cgroup, &request.background_cgroup] {
            required_cgroup_files.insert(format!("{cgroup}/cgroup.procs"));
            required_cgroup_files.insert(format!("{cgroup}/cpuset.cpus"));
            required_cgroup_files.insert(format!("{cgroup}/cpuset.mems"));
            required_cgroup_files.insert(format!("{cgroup}/cpu.max"));
        }

        Self {
            required_membership_pids: all_pids(request),
            required_cgroup_files: required_cgroup_files.into_iter().collect(),
        }
    }
}

impl CpusetRollbackPlan {
    fn from_capture(capture: &CpusetRollbackCapture, request: &CpusetDryRunRequest) -> Self {
        let mut restore_cgroup_files = BTreeSet::new();
        for cgroup in [&request.protected_cgroup, &request.background_cgroup] {
            restore_cgroup_files.insert(format!("{cgroup}/cpuset.cpus"));
            restore_cgroup_files.insert(format!("{cgroup}/cpuset.mems"));
            if capture.original_cpu_max.contains_key(cgroup) {
                restore_cgroup_files.insert(format!("{cgroup}/cpu.max"));
            }
        }

        Self {
            restore_membership_pids: all_pids(request)
                .into_iter()
                .filter(|pid| capture.original_memberships.contains_key(pid))
                .collect(),
            restore_cgroup_files: restore_cgroup_files.into_iter().collect(),
            remove_temporary_cgroups: capture.temporary_cgroups.clone(),
        }
    }
}

fn first_rejection(
    request: &CpusetDryRunRequest,
    target_context: &CpusetDryRunTargetContext,
) -> Option<(CpusetDryRunRejectionReason, String)> {
    if request.mode == CpusetDryRunMode::LiveWrite {
        return Some((
            CpusetDryRunRejectionReason::UnsupportedLiveWriteMode,
            "live cpuset/background writes are disabled; dry-run planner only".to_string(),
        ));
    }

    if !is_safe_cgroup_root(&request.cgroup_root) {
        return Some((
            CpusetDryRunRejectionReason::UnsafeCgroupRoot,
            format!(
                "cgroup root `{}` is outside `{AEGISAI_CGROUP_ROOT}`",
                request.cgroup_root
            ),
        ));
    }
    for cgroup in [&request.protected_cgroup, &request.background_cgroup] {
        if !is_safe_child_cgroup(&request.cgroup_root, cgroup) {
            return Some((
                CpusetDryRunRejectionReason::UnsafeCgroupRoot,
                format!(
                    "target cgroup `{cgroup}` is outside owned root `{}`",
                    request.cgroup_root
                ),
            ));
        }
    }

    if let Some(detail) = classification_rejection(request) {
        return Some((
            CpusetDryRunRejectionReason::MissingProcessClassification,
            detail,
        ));
    }

    if target_context.proposed_cpus.is_empty() {
        return Some((
            CpusetDryRunRejectionReason::EmptyComputedCpuSet,
            format!(
                "computed cpuset CPU set is empty for `{}`",
                request.protected_cgroup
            ),
        ));
    }

    if let Some(detail) = rollback_capture_rejection(request) {
        return Some((CpusetDryRunRejectionReason::MissingRollbackCapture, detail));
    }

    if target_context.affected_process_count > request.max_processes {
        return Some((
            CpusetDryRunRejectionReason::OverbroadProcessSet,
            format!(
                "affected process count {} exceeds maximum {}",
                target_context.affected_process_count, request.max_processes
            ),
        ));
    }

    None
}

fn classification_rejection(request: &CpusetDryRunRequest) -> Option<String> {
    for target in &request.protected_targets {
        match target.classification {
            Some(CpusetProcessClassification::InteractiveAiInference)
            | Some(CpusetProcessClassification::ActiveToolCall) => {}
            Some(classification) => {
                return Some(format!(
                    "pid {} (`{}`) classification `{}` is not eligible as protected work",
                    target.pid,
                    target.process_name,
                    classification.as_str()
                ));
            }
            None => {
                return Some(format!(
                    "pid {} (`{}`) has no cpuset/background classification",
                    target.pid, target.process_name
                ));
            }
        }
    }

    for target in &request.background_targets {
        match target.classification {
            Some(CpusetProcessClassification::BackgroundJob) => {}
            Some(classification) => {
                return Some(format!(
                    "pid {} (`{}`) classification `{}` is not eligible as background work",
                    target.pid,
                    target.process_name,
                    classification.as_str()
                ));
            }
            None => {
                return Some(format!(
                    "pid {} (`{}`) has no cpuset/background classification",
                    target.pid, target.process_name
                ));
            }
        }
    }

    None
}

fn rollback_capture_rejection(request: &CpusetDryRunRequest) -> Option<String> {
    let Some(capture) = request.rollback_capture.as_ref() else {
        return Some(
            "rollback capture is required before cpuset/background isolation planning".to_string(),
        );
    };

    for pid in all_pids(request) {
        if !capture.original_memberships.contains_key(&pid) {
            return Some(format!(
                "rollback capture is missing original cgroup for pid {pid}"
            ));
        }
    }

    for cgroup in [&request.protected_cgroup, &request.background_cgroup] {
        if !capture.original_cpuset_cpus.contains_key(cgroup) {
            return Some(format!(
                "rollback capture is missing original cpuset.cpus for `{cgroup}`"
            ));
        }
        if !capture.original_cpuset_mems.contains_key(cgroup) {
            return Some(format!(
                "rollback capture is missing original cpuset.mems for `{cgroup}`"
            ));
        }
    }

    None
}

fn is_safe_cgroup_root(root: &str) -> bool {
    let trimmed = trim_trailing_slash(root);
    if trimmed.is_empty() || has_parent_dir(&trimmed) {
        return false;
    }

    trimmed == AEGISAI_CGROUP_ROOT || trimmed.starts_with(&format!("{AEGISAI_CGROUP_ROOT}/"))
}

fn is_safe_child_cgroup(root: &str, cgroup: &str) -> bool {
    let root = trim_trailing_slash(root);
    let cgroup = trim_trailing_slash(cgroup);
    !has_parent_dir(&cgroup) && cgroup.starts_with(&format!("{root}/"))
}

fn trim_trailing_slash(value: &str) -> String {
    let trimmed = value.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        String::new()
    } else {
        trimmed.to_string()
    }
}

fn has_parent_dir(value: &str) -> bool {
    Path::new(value)
        .components()
        .any(|component| matches!(component, Component::ParentDir))
}

fn all_pids(request: &CpusetDryRunRequest) -> Vec<u32> {
    let mut pids = BTreeSet::new();
    for target in request
        .protected_targets
        .iter()
        .chain(request.background_targets.iter())
    {
        pids.insert(target.pid);
    }
    pids.into_iter().collect()
}

fn sorted_pids(targets: &[CpusetProcessTarget]) -> Vec<u32> {
    targets
        .iter()
        .map(|target| target.pid)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn normalized_cpus(cpus: &[u32]) -> Vec<u32> {
    cpus.iter()
        .copied()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{
        plan_cpuset_dry_run, CpusetDryRunMode, CpusetDryRunRejectionReason, CpusetDryRunRequest,
        CpusetProcessClassification, CpusetProcessTarget, CpusetRollbackCapture,
    };

    fn valid_request() -> CpusetDryRunRequest {
        let protected_cgroup = "/sys/fs/cgroup/aegisai.runtime/protected".to_string();
        let background_cgroup = "/sys/fs/cgroup/aegisai.runtime/background".to_string();
        let mut capture = CpusetRollbackCapture::default();
        capture
            .original_memberships
            .insert(42, "/sys/fs/cgroup/aegisai.runtime/protected".to_string());
        capture
            .original_memberships
            .insert(84, "/sys/fs/cgroup/aegisai.runtime/background".to_string());
        capture
            .original_cpuset_cpus
            .insert(protected_cgroup.clone(), "0-3".to_string());
        capture
            .original_cpuset_cpus
            .insert(background_cgroup.clone(), "4-7".to_string());
        capture
            .original_cpuset_mems
            .insert(protected_cgroup.clone(), "0".to_string());
        capture
            .original_cpuset_mems
            .insert(background_cgroup.clone(), "0".to_string());
        capture
            .original_cpu_max
            .insert(background_cgroup.clone(), "max 100000".to_string());
        capture
            .temporary_cgroups
            .push("/sys/fs/cgroup/aegisai.runtime/background.tmp".to_string());

        CpusetDryRunRequest {
            mode: CpusetDryRunMode::DryRun,
            cgroup_root: "/sys/fs/cgroup/aegisai.runtime".to_string(),
            protected_cgroup,
            background_cgroup,
            proposed_cpus: vec![3, 1, 1],
            cpuset_mems: Some("0".to_string()),
            protected_targets: vec![CpusetProcessTarget::new(
                42,
                "ollama",
                Some(CpusetProcessClassification::InteractiveAiInference),
            )
            .with_cgroup("/sys/fs/cgroup/aegisai.runtime/protected")],
            background_targets: vec![CpusetProcessTarget::new(
                84,
                "python-batch",
                Some(CpusetProcessClassification::BackgroundJob),
            )
            .with_cgroup("/sys/fs/cgroup/aegisai.runtime/background")],
            rollback_capture: Some(capture),
            max_processes: 8,
        }
    }

    #[test]
    fn valid_dry_run_plan_includes_target_capture_and_rollback_context() {
        let plan = plan_cpuset_dry_run(valid_request());

        assert!(plan.accepted());
        assert_eq!(plan.target_context.proposed_cpus, vec![1, 3]);
        assert_eq!(plan.target_context.protected_pids, vec![42]);
        assert_eq!(plan.target_context.background_pids, vec![84]);
        assert_eq!(plan.capture_plan.required_membership_pids, vec![42, 84]);
        assert!(plan
            .capture_plan
            .required_cgroup_files
            .contains(&"/sys/fs/cgroup/aegisai.runtime/protected/cpuset.cpus".to_string()));

        let rollback_plan = plan.rollback_plan.expect("rollback plan");
        assert_eq!(rollback_plan.restore_membership_pids, vec![42, 84]);
        assert!(rollback_plan
            .restore_cgroup_files
            .contains(&"/sys/fs/cgroup/aegisai.runtime/background/cpu.max".to_string()));
        assert_eq!(
            rollback_plan.remove_temporary_cgroups,
            vec!["/sys/fs/cgroup/aegisai.runtime/background.tmp".to_string()]
        );
    }

    #[test]
    fn rejects_unsupported_live_write_mode() {
        let mut request = valid_request();
        request.mode = CpusetDryRunMode::LiveWrite;

        let plan = plan_cpuset_dry_run(request);
        let rejection = plan.rejection.expect("rejection");

        assert_eq!(
            rejection.reason,
            CpusetDryRunRejectionReason::UnsupportedLiveWriteMode
        );
        assert_eq!(rejection.reason.as_str(), "unsupported_live_write_mode");
        assert_eq!(
            rejection.detail,
            "live cpuset/background writes are disabled; dry-run planner only"
        );
        assert_eq!(rejection.target_context.protected_pids, vec![42]);
        assert_eq!(rejection.target_context.background_pids, vec![84]);
    }

    #[test]
    fn rejects_unsafe_cgroup_root() {
        let mut request = valid_request();
        request.cgroup_root = "/sys/fs/cgroup".to_string();

        let plan = plan_cpuset_dry_run(request);
        let rejection = plan.rejection.expect("rejection");

        assert_eq!(
            rejection.reason,
            CpusetDryRunRejectionReason::UnsafeCgroupRoot
        );
        assert_eq!(rejection.reason.as_str(), "unsafe_cgroup_root");
        assert_eq!(
            rejection.detail,
            "cgroup root `/sys/fs/cgroup` is outside `/sys/fs/cgroup/aegisai.runtime`"
        );
        assert_eq!(rejection.target_context.cgroup_root, "/sys/fs/cgroup");
        assert_eq!(rejection.target_context.protected_pids, vec![42]);
    }

    #[test]
    fn rejects_target_cgroup_at_owned_root() {
        let mut request = valid_request();
        request.protected_cgroup = "/sys/fs/cgroup/aegisai.runtime".to_string();

        let plan = plan_cpuset_dry_run(request);
        let rejection = plan.rejection.expect("rejection");

        assert_eq!(
            rejection.reason,
            CpusetDryRunRejectionReason::UnsafeCgroupRoot
        );
        assert_eq!(
            rejection.detail,
            "target cgroup `/sys/fs/cgroup/aegisai.runtime` is outside owned root `/sys/fs/cgroup/aegisai.runtime`"
        );
        assert_eq!(
            rejection.target_context.protected_cgroup,
            "/sys/fs/cgroup/aegisai.runtime"
        );
    }

    #[test]
    fn rejects_missing_process_classification() {
        let mut request = valid_request();
        request.background_targets[0].classification = None;

        let plan = plan_cpuset_dry_run(request);
        let rejection = plan.rejection.expect("rejection");

        assert_eq!(
            rejection.reason,
            CpusetDryRunRejectionReason::MissingProcessClassification
        );
        assert_eq!(rejection.reason.as_str(), "missing_process_classification");
        assert_eq!(
            rejection.detail,
            "pid 84 (`python-batch`) has no cpuset/background classification"
        );
        assert_eq!(rejection.target_context.background_pids, vec![84]);
    }

    #[test]
    fn rejects_empty_computed_cpu_set() {
        let mut request = valid_request();
        request.proposed_cpus.clear();

        let plan = plan_cpuset_dry_run(request);
        let rejection = plan.rejection.expect("rejection");

        assert_eq!(
            rejection.reason,
            CpusetDryRunRejectionReason::EmptyComputedCpuSet
        );
        assert_eq!(rejection.reason.as_str(), "empty_computed_cpu_set");
        assert_eq!(
            rejection.detail,
            "computed cpuset CPU set is empty for `/sys/fs/cgroup/aegisai.runtime/protected`"
        );
        assert!(rejection.target_context.proposed_cpus.is_empty());
    }

    #[test]
    fn rejects_missing_rollback_capture() {
        let mut request = valid_request();
        request.rollback_capture = None;

        let plan = plan_cpuset_dry_run(request);
        let rejection = plan.rejection.expect("rejection");

        assert_eq!(
            rejection.reason,
            CpusetDryRunRejectionReason::MissingRollbackCapture
        );
        assert_eq!(rejection.reason.as_str(), "missing_rollback_capture");
        assert_eq!(
            rejection.detail,
            "rollback capture is required before cpuset/background isolation planning"
        );
        assert_eq!(rejection.target_context.affected_process_count, 2);
        assert!(plan.rollback_plan.is_none());
        assert_eq!(plan.capture_plan.required_membership_pids, vec![42, 84]);
    }

    #[test]
    fn rejects_overbroad_process_set() {
        let mut request = valid_request();
        request.max_processes = 1;

        let plan = plan_cpuset_dry_run(request);
        let rejection = plan.rejection.expect("rejection");

        assert_eq!(
            rejection.reason,
            CpusetDryRunRejectionReason::OverbroadProcessSet
        );
        assert_eq!(rejection.reason.as_str(), "overbroad_process_set");
        assert_eq!(
            rejection.detail,
            "affected process count 2 exceeds maximum 1"
        );
        assert_eq!(rejection.target_context.affected_process_count, 2);
        assert_eq!(rejection.target_context.protected_pids, vec![42]);
        assert_eq!(rejection.target_context.background_pids, vec![84]);
    }
}
