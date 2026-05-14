use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path};

use crate::backend::BackendExecution;
use crate::cpuset_dry_run::{CpusetProcessClassification, CpusetProcessTarget};

const DEFAULT_AEGISAI_CGROUP_ROOT: &str = "/sys/fs/cgroup/aegisai.runtime";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OwnedCgroupIsolationGuard {
    owned_root: String,
    allowed_pids: BTreeSet<u32>,
    forbidden_pids: BTreeSet<u32>,
    explicit_confirmation: bool,
    max_processes: usize,
}

impl OwnedCgroupIsolationGuard {
    pub fn new<I>(
        owned_root: impl Into<String>,
        allowed_pids: I,
        explicit_confirmation: bool,
    ) -> Self
    where
        I: IntoIterator<Item = u32>,
    {
        Self {
            owned_root: normalize_cgroup_path(&owned_root.into()),
            allowed_pids: allowed_pids.into_iter().collect(),
            forbidden_pids: BTreeSet::new(),
            explicit_confirmation,
            max_processes: 16,
        }
    }

    pub fn aegisai_default<I>(allowed_pids: I, explicit_confirmation: bool) -> Self
    where
        I: IntoIterator<Item = u32>,
    {
        Self::new(
            DEFAULT_AEGISAI_CGROUP_ROOT,
            allowed_pids,
            explicit_confirmation,
        )
    }

    pub fn with_forbidden_pids<I>(mut self, forbidden_pids: I) -> Self
    where
        I: IntoIterator<Item = u32>,
    {
        self.forbidden_pids = forbidden_pids.into_iter().collect();
        self
    }

    pub fn with_max_processes(mut self, max_processes: usize) -> Self {
        self.max_processes = max_processes;
        self
    }

    pub fn owned_root(&self) -> &str {
        &self.owned_root
    }

    fn validate_pid(&self, pid: u32) -> Result<(), String> {
        if pid == 0 {
            return Err("owned cgroup isolation refuses pid 0".to_string());
        }
        if self.forbidden_pids.contains(&pid) {
            return Err(format!(
                "owned cgroup isolation refuses daemon/helper/self pid {pid}"
            ));
        }
        if !self.allowed_pids.contains(&pid) {
            return Err(format!(
                "pid {pid} is not in owned cgroup isolation PID allowlist"
            ));
        }

        Ok(())
    }

    fn allowed_pids_label(&self) -> String {
        self.allowed_pids
            .iter()
            .map(u32::to_string)
            .collect::<Vec<_>>()
            .join(",")
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OwnedCgroupIsolationRequest {
    pub cgroup_root: String,
    pub protected_cgroup: String,
    pub background_cgroup: Option<String>,
    pub protected_cpuset_cpus: String,
    pub background_cpuset_cpus: Option<String>,
    pub cpuset_mems: Option<String>,
    pub protected_cpu_max: Option<String>,
    pub background_cpu_max: Option<String>,
    pub protected_targets: Vec<CpusetProcessTarget>,
    pub background_targets: Vec<CpusetProcessTarget>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OwnedCgroupIsolationLease {
    pub original_memberships: BTreeMap<u32, String>,
    pub original_cpuset_cpus: BTreeMap<String, String>,
    pub original_cpuset_mems: BTreeMap<String, String>,
    pub original_cpu_max: BTreeMap<String, String>,
    pub original_cgroup_procs: BTreeMap<String, String>,
    pub touched_cgroups: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OwnedCgroupIsolationApplyResult {
    pub execution: BackendExecution,
    pub lease: Option<OwnedCgroupIsolationLease>,
}

pub trait CgroupFs {
    fn is_dir(&self, path: &str) -> bool;

    fn read_to_string(&self, path: &str) -> Result<String, String>;

    fn write_string(&mut self, path: &str, value: &str) -> Result<(), String>;
}

#[derive(Default)]
pub struct SystemCgroupFs;

impl CgroupFs for SystemCgroupFs {
    fn is_dir(&self, path: &str) -> bool {
        Path::new(path).is_dir()
    }

    fn read_to_string(&self, path: &str) -> Result<String, String> {
        std::fs::read_to_string(path).map_err(|error| format!("read `{path}` failed: {error}"))
    }

    fn write_string(&mut self, path: &str, value: &str) -> Result<(), String> {
        std::fs::write(path, value).map_err(|error| format!("write `{path}` failed: {error}"))
    }
}

pub struct OwnedCgroupIsolationApplier {
    guard: OwnedCgroupIsolationGuard,
    fs: Box<dyn CgroupFs>,
    disabled: bool,
}

impl OwnedCgroupIsolationApplier {
    pub fn new(guard: OwnedCgroupIsolationGuard) -> Self {
        Self::with_fs(guard, SystemCgroupFs)
    }

    pub fn with_fs<F>(guard: OwnedCgroupIsolationGuard, fs: F) -> Self
    where
        F: CgroupFs + 'static,
    {
        Self {
            guard,
            fs: Box::new(fs),
            disabled: false,
        }
    }

    pub fn disabled(&self) -> bool {
        self.disabled
    }

    pub fn apply(
        &mut self,
        request: &OwnedCgroupIsolationRequest,
        now_ms: u64,
    ) -> OwnedCgroupIsolationApplyResult {
        let mut execution = self.base_execution("apply", now_ms);
        if self.disabled {
            return OwnedCgroupIsolationApplyResult {
                execution: execution
                    .with_field("apply.result", "disabled")
                    .with_field("apply.error", "owned cgroup isolation applier is disabled"),
                lease: None,
            };
        }

        if let Err(error) = self.validate_request(request) {
            return OwnedCgroupIsolationApplyResult {
                execution: execution
                    .with_field("apply.result", "rejected")
                    .with_field("apply.error", error)
                    .with_field("apply.write_count", "0"),
                lease: None,
            };
        }

        let lease = match self.capture_lease(request) {
            Ok(lease) => lease,
            Err(error) => {
                return OwnedCgroupIsolationApplyResult {
                    execution: execution
                        .with_field("apply.result", "rejected")
                        .with_field("apply.error", error)
                        .with_field("apply.write_count", "0"),
                    lease: None,
                };
            }
        };

        execution = annotate_lease_capture(execution, &lease);
        match self.apply_writes(request, &mut execution) {
            Ok(write_count) => OwnedCgroupIsolationApplyResult {
                execution: execution
                    .with_field("apply.result", "ok")
                    .with_field("apply.write_count", write_count.to_string())
                    .with_field(
                        "apply.moved_pid_count",
                        lease.original_memberships.len().to_string(),
                    )
                    .with_field("side_effect", "cgroupfs_write"),
                lease: Some(lease),
            },
            Err(error) => {
                self.disabled = true;
                let rollback_execution = self.rollback_internal(&lease, now_ms, "failure_rollback");
                execution =
                    merge_prefixed_execution(execution, "failure_rollback", rollback_execution);
                OwnedCgroupIsolationApplyResult {
                    execution: execution
                        .with_field("apply.result", "error")
                        .with_field("apply.error", error)
                        .with_field("apply.failure_disabled", "true")
                        .with_field("side_effect", "cgroupfs_write_partial"),
                    lease: None,
                }
            }
        }
    }

    pub fn rollback(&mut self, lease: &OwnedCgroupIsolationLease, now_ms: u64) -> BackendExecution {
        self.rollback_internal(lease, now_ms, "rollback")
    }

    fn base_execution(&self, phase: &str, now_ms: u64) -> BackendExecution {
        BackendExecution::default()
            .with_field("applier", "owned-cgroup-isolation")
            .with_field("phase", phase)
            .with_field("timestamp_ms", now_ms.to_string())
            .with_field(
                "guard.confirmed",
                self.guard.explicit_confirmation.to_string(),
            )
            .with_field("guard.owned_root", self.guard.owned_root.clone())
            .with_field("guard.allowed_pids", self.guard.allowed_pids_label())
            .with_field("guard.max_processes", self.guard.max_processes.to_string())
            .with_field("guard.disabled", self.disabled.to_string())
    }

    fn validate_request(&self, request: &OwnedCgroupIsolationRequest) -> Result<(), String> {
        if !self.guard.explicit_confirmation {
            return Err("owned cgroup isolation requires explicit live confirmation".to_string());
        }
        if self.guard.allowed_pids.is_empty() {
            return Err("owned cgroup isolation requires a non-empty PID allowlist".to_string());
        }
        if !is_under_or_equal(&self.guard.owned_root, DEFAULT_AEGISAI_CGROUP_ROOT)
            || has_parent_dir(&self.guard.owned_root)
            || has_rejected_segment(&self.guard.owned_root)
        {
            return Err(format!(
                "configured owned root `{}` is outside `{DEFAULT_AEGISAI_CGROUP_ROOT}`",
                self.guard.owned_root
            ));
        }

        let cgroup_root = normalize_cgroup_path(&request.cgroup_root);
        if has_parent_dir(&cgroup_root) {
            return Err(format!(
                "cgroup root `{cgroup_root}` contains parent directory traversal"
            ));
        }
        if has_rejected_segment(&cgroup_root) {
            return Err(format!(
                "cgroup root `{cgroup_root}` contains system/container scope"
            ));
        }
        if !is_under_or_equal(&cgroup_root, &self.guard.owned_root) {
            return Err(format!(
                "cgroup root `{cgroup_root}` is outside owned root `{}`",
                self.guard.owned_root
            ));
        }
        if !self.fs.is_dir(&cgroup_root) {
            return Err(format!(
                "owned cgroup root `{cgroup_root}` must already exist"
            ));
        }

        validate_owned_child_cgroup(&cgroup_root, &request.protected_cgroup)?;
        if !self
            .fs
            .is_dir(&normalize_cgroup_path(&request.protected_cgroup))
        {
            return Err(format!(
                "protected cgroup `{}` must already exist",
                normalize_cgroup_path(&request.protected_cgroup)
            ));
        }
        if request.protected_cpuset_cpus.trim().is_empty() {
            return Err("protected cpuset.cpus must be non-empty".to_string());
        }
        if let Some(background_cgroup) = &request.background_cgroup {
            validate_owned_child_cgroup(&cgroup_root, background_cgroup)?;
            if !self.fs.is_dir(&normalize_cgroup_path(background_cgroup)) {
                return Err(format!(
                    "background cgroup `{}` must already exist",
                    normalize_cgroup_path(background_cgroup)
                ));
            }
            if request
                .background_cpuset_cpus
                .as_deref()
                .is_some_and(|value| value.trim().is_empty())
            {
                return Err("background cpuset.cpus must be non-empty when set".to_string());
            }
        } else if !request.background_targets.is_empty()
            || request.background_cpuset_cpus.is_some()
            || request.background_cpu_max.is_some()
        {
            return Err("background isolation settings require a background cgroup".to_string());
        }

        let affected = request.protected_targets.len() + request.background_targets.len();
        if affected == 0 {
            return Err("owned cgroup isolation requires at least one target pid".to_string());
        }
        if affected > self.guard.max_processes {
            return Err(format!(
                "affected process count {affected} exceeds maximum {}",
                self.guard.max_processes
            ));
        }

        let mut seen = BTreeSet::new();
        for target in &request.protected_targets {
            self.validate_target(
                target,
                &cgroup_root,
                &mut seen,
                "protected",
                &[
                    CpusetProcessClassification::InteractiveAiInference,
                    CpusetProcessClassification::ActiveToolCall,
                ],
            )?;
        }
        for target in &request.background_targets {
            self.validate_target(
                target,
                &cgroup_root,
                &mut seen,
                "background",
                &[CpusetProcessClassification::BackgroundJob],
            )?;
        }

        Ok(())
    }

    fn validate_target(
        &self,
        target: &CpusetProcessTarget,
        cgroup_root: &str,
        seen: &mut BTreeSet<u32>,
        role: &str,
        allowed_classifications: &[CpusetProcessClassification],
    ) -> Result<(), String> {
        self.guard.validate_pid(target.pid)?;
        if !seen.insert(target.pid) {
            return Err(format!(
                "pid {} appears in multiple owned cgroup isolation roles",
                target.pid
            ));
        }

        let Some(classification) = target.classification else {
            return Err(format!(
                "pid {} (`{}`) has no owned cgroup isolation classification",
                target.pid, target.process_name
            ));
        };
        if !allowed_classifications.contains(&classification) {
            return Err(format!(
                "pid {} (`{}`) classification `{}` is not eligible as {role} work",
                target.pid,
                target.process_name,
                classification.as_str()
            ));
        }

        let Some(cgroup) = target.cgroup.as_ref() else {
            return Err(format!(
                "pid {} (`{}`) is missing original cgroup membership",
                target.pid, target.process_name
            ));
        };
        let cgroup = normalize_membership_path(cgroup);
        if is_rejected_original_cgroup(&cgroup) || has_parent_dir(&cgroup) {
            return Err(format!(
                "pid {} (`{}`) original cgroup `{cgroup}` is root/system/container scope",
                target.pid, target.process_name
            ));
        }
        if !is_strict_child_of(&cgroup, cgroup_root) {
            return Err(format!(
                "pid {} (`{}`) original cgroup `{cgroup}` is outside owned root `{cgroup_root}`",
                target.pid, target.process_name
            ));
        }
        if !self.fs.is_dir(&cgroup) {
            return Err(format!(
                "pid {} (`{}`) original cgroup `{cgroup}` must already exist",
                target.pid, target.process_name
            ));
        }

        Ok(())
    }

    fn capture_lease(
        &self,
        request: &OwnedCgroupIsolationRequest,
    ) -> Result<OwnedCgroupIsolationLease, String> {
        let mut lease = OwnedCgroupIsolationLease::default();
        for target in request
            .protected_targets
            .iter()
            .chain(request.background_targets.iter())
        {
            let original = target
                .cgroup
                .as_ref()
                .map(|cgroup| normalize_membership_path(cgroup))
                .ok_or_else(|| {
                    format!(
                        "pid {} (`{}`) is missing original cgroup membership",
                        target.pid, target.process_name
                    )
                })?;
            lease.original_memberships.insert(target.pid, original);
        }

        for cgroup in request.target_cgroups() {
            capture_cgroup_file(
                self.fs.as_ref(),
                &mut lease.original_cpuset_cpus,
                &cgroup,
                "cpuset.cpus",
            )?;
            capture_cgroup_file(
                self.fs.as_ref(),
                &mut lease.original_cpuset_mems,
                &cgroup,
                "cpuset.mems",
            )?;
            capture_cgroup_file(
                self.fs.as_ref(),
                &mut lease.original_cpu_max,
                &cgroup,
                "cpu.max",
            )?;
            capture_cgroup_file(
                self.fs.as_ref(),
                &mut lease.original_cgroup_procs,
                &cgroup,
                "cgroup.procs",
            )?;
            lease.touched_cgroups.push(cgroup);
        }

        Ok(lease)
    }

    fn apply_writes(
        &mut self,
        request: &OwnedCgroupIsolationRequest,
        execution: &mut BackendExecution,
    ) -> Result<usize, String> {
        let mut write_index = 0usize;
        let protected_cgroup = normalize_cgroup_path(&request.protected_cgroup);
        if let Some(mems) = request.cpuset_mems.as_deref() {
            self.write_audited(
                execution,
                &mut write_index,
                &cgroup_file(&protected_cgroup, "cpuset.mems"),
                mems,
                "protected_cpuset_mems",
            )?;
        }
        self.write_audited(
            execution,
            &mut write_index,
            &cgroup_file(&protected_cgroup, "cpuset.cpus"),
            request.protected_cpuset_cpus.trim(),
            "protected_cpuset_cpus",
        )?;
        if let Some(cpu_max) = request.protected_cpu_max.as_deref() {
            self.write_audited(
                execution,
                &mut write_index,
                &cgroup_file(&protected_cgroup, "cpu.max"),
                cpu_max,
                "protected_cpu_max",
            )?;
        }

        if let Some(background_cgroup) = request.background_cgroup.as_ref() {
            let background_cgroup = normalize_cgroup_path(background_cgroup);
            if let Some(mems) = request.cpuset_mems.as_deref() {
                self.write_audited(
                    execution,
                    &mut write_index,
                    &cgroup_file(&background_cgroup, "cpuset.mems"),
                    mems,
                    "background_cpuset_mems",
                )?;
            }
            if let Some(cpus) = request.background_cpuset_cpus.as_deref() {
                self.write_audited(
                    execution,
                    &mut write_index,
                    &cgroup_file(&background_cgroup, "cpuset.cpus"),
                    cpus,
                    "background_cpuset_cpus",
                )?;
            }
            if let Some(cpu_max) = request.background_cpu_max.as_deref() {
                self.write_audited(
                    execution,
                    &mut write_index,
                    &cgroup_file(&background_cgroup, "cpu.max"),
                    cpu_max,
                    "background_cpu_max",
                )?;
            }
        }

        for target in &request.protected_targets {
            self.write_audited(
                execution,
                &mut write_index,
                &cgroup_file(&protected_cgroup, "cgroup.procs"),
                &target.pid.to_string(),
                "move_protected_pid",
            )?;
        }
        if let Some(background_cgroup) = request.background_cgroup.as_ref() {
            let background_cgroup = normalize_cgroup_path(background_cgroup);
            for target in &request.background_targets {
                self.write_audited(
                    execution,
                    &mut write_index,
                    &cgroup_file(&background_cgroup, "cgroup.procs"),
                    &target.pid.to_string(),
                    "move_background_pid",
                )?;
            }
        }

        Ok(write_index)
    }

    fn write_audited(
        &mut self,
        execution: &mut BackendExecution,
        write_index: &mut usize,
        path: &str,
        value: &str,
        role: &str,
    ) -> Result<(), String> {
        let current_index = *write_index;
        *write_index += 1;
        match self.fs.write_string(path, value.trim()) {
            Ok(()) => {
                *execution = std::mem::take(execution)
                    .with_field(format!("write.{current_index}.status"), "ok")
                    .with_field(format!("write.{current_index}.role"), role)
                    .with_field(format!("write.{current_index}.path"), path)
                    .with_field(format!("write.{current_index}.value"), value.trim());
                Ok(())
            }
            Err(error) => {
                *execution = std::mem::take(execution)
                    .with_field(format!("write.{current_index}.status"), "error")
                    .with_field(format!("write.{current_index}.role"), role)
                    .with_field(format!("write.{current_index}.path"), path)
                    .with_field(format!("write.{current_index}.error"), error.clone());
                Err(error)
            }
        }
    }

    fn rollback_internal(
        &mut self,
        lease: &OwnedCgroupIsolationLease,
        now_ms: u64,
        phase: &str,
    ) -> BackendExecution {
        let mut execution = self.base_execution(phase, now_ms);
        let mut restored = 0usize;
        let mut failed = Vec::new();
        let mut write_index = 0usize;

        for (pid, original_cgroup) in &lease.original_memberships {
            let path = cgroup_file(original_cgroup, "cgroup.procs");
            if let Err(error) =
                validate_owned_rollback_cgroup(original_cgroup, &self.guard.owned_root)
            {
                failed.push(format!("{path}:{error}"));
                execution = execution
                    .with_field(format!("rollback.{write_index}.status"), "error")
                    .with_field(format!("rollback.{write_index}.role"), "restore_pid")
                    .with_field(format!("rollback.{write_index}.path"), path)
                    .with_field(format!("rollback.{write_index}.error"), error);
                write_index += 1;
                continue;
            }
            match self.fs.write_string(&path, &pid.to_string()) {
                Ok(()) => {
                    restored += 1;
                    execution = execution
                        .with_field(format!("rollback.{write_index}.status"), "ok")
                        .with_field(format!("rollback.{write_index}.role"), "restore_pid")
                        .with_field(format!("rollback.{write_index}.path"), path)
                        .with_field(format!("rollback.{write_index}.value"), pid.to_string());
                }
                Err(error) => {
                    failed.push(format!("{path}:{error}"));
                    execution = execution
                        .with_field(format!("rollback.{write_index}.status"), "error")
                        .with_field(format!("rollback.{write_index}.role"), "restore_pid")
                        .with_field(format!("rollback.{write_index}.path"), path)
                        .with_field(format!("rollback.{write_index}.error"), error);
                }
            }
            write_index += 1;
        }

        for cgroup in &lease.touched_cgroups {
            for (file, values, role) in [
                (
                    "cpuset.mems",
                    &lease.original_cpuset_mems,
                    "restore_cpuset_mems",
                ),
                (
                    "cpuset.cpus",
                    &lease.original_cpuset_cpus,
                    "restore_cpuset_cpus",
                ),
                ("cpu.max", &lease.original_cpu_max, "restore_cpu_max"),
            ] {
                let path = cgroup_file(cgroup, file);
                let value = values.get(cgroup).cloned().unwrap_or_default();
                if let Err(error) = validate_owned_rollback_cgroup(cgroup, &self.guard.owned_root) {
                    failed.push(format!("{path}:{error}"));
                    execution = execution
                        .with_field(format!("rollback.{write_index}.status"), "error")
                        .with_field(format!("rollback.{write_index}.role"), role)
                        .with_field(format!("rollback.{write_index}.path"), path)
                        .with_field(format!("rollback.{write_index}.error"), error);
                    write_index += 1;
                    continue;
                }
                match self.fs.write_string(&path, &value) {
                    Ok(()) => {
                        restored += 1;
                        execution = execution
                            .with_field(format!("rollback.{write_index}.status"), "ok")
                            .with_field(format!("rollback.{write_index}.role"), role)
                            .with_field(format!("rollback.{write_index}.path"), path)
                            .with_field(format!("rollback.{write_index}.value"), value);
                    }
                    Err(error) => {
                        failed.push(format!("{path}:{error}"));
                        execution = execution
                            .with_field(format!("rollback.{write_index}.status"), "error")
                            .with_field(format!("rollback.{write_index}.role"), role)
                            .with_field(format!("rollback.{write_index}.path"), path)
                            .with_field(format!("rollback.{write_index}.error"), error);
                    }
                }
                write_index += 1;
            }
        }

        let attempted = write_index;
        let failed_count = failed.len();
        let success_rate = if attempted == 0 {
            "1.000".to_string()
        } else {
            format!(
                "{:.3}",
                (attempted - failed_count) as f64 / attempted as f64
            )
        };
        execution = execution
            .with_field("rollback.attempted_count", attempted.to_string())
            .with_field("rollback.restored_count", restored.to_string())
            .with_field("rollback.failed_count", failed_count.to_string())
            .with_field("rollback.success_rate", success_rate)
            .with_field(
                "rollback.result",
                if failed.is_empty() { "ok" } else { "error" },
            );
        if !failed.is_empty() {
            execution = execution.with_field("rollback.failed", failed.join(","));
        }

        execution
    }
}

impl OwnedCgroupIsolationRequest {
    fn target_cgroups(&self) -> Vec<String> {
        let mut cgroups = BTreeSet::new();
        cgroups.insert(normalize_cgroup_path(&self.protected_cgroup));
        if let Some(background_cgroup) = &self.background_cgroup {
            cgroups.insert(normalize_cgroup_path(background_cgroup));
        }
        cgroups.into_iter().collect()
    }
}

fn annotate_lease_capture(
    mut execution: BackendExecution,
    lease: &OwnedCgroupIsolationLease,
) -> BackendExecution {
    execution = execution
        .with_field(
            "capture.membership_count",
            lease.original_memberships.len().to_string(),
        )
        .with_field(
            "capture.cgroup_count",
            lease.touched_cgroups.len().to_string(),
        );
    for (pid, cgroup) in &lease.original_memberships {
        execution = execution.with_field(format!("capture.pid.{pid}.original_cgroup"), cgroup);
    }
    for (index, cgroup) in lease.touched_cgroups.iter().enumerate() {
        execution = execution
            .with_field(format!("capture.cgroup.{index}.path"), cgroup)
            .with_field(
                format!("capture.cgroup.{index}.cpuset_cpus"),
                lease
                    .original_cpuset_cpus
                    .get(cgroup)
                    .cloned()
                    .unwrap_or_default(),
            )
            .with_field(
                format!("capture.cgroup.{index}.cpuset_mems"),
                lease
                    .original_cpuset_mems
                    .get(cgroup)
                    .cloned()
                    .unwrap_or_default(),
            )
            .with_field(
                format!("capture.cgroup.{index}.cpu_max"),
                lease
                    .original_cpu_max
                    .get(cgroup)
                    .cloned()
                    .unwrap_or_default(),
            )
            .with_field(
                format!("capture.cgroup.{index}.procs"),
                lease
                    .original_cgroup_procs
                    .get(cgroup)
                    .cloned()
                    .unwrap_or_default(),
            );
    }
    execution
}

fn capture_cgroup_file(
    fs: &dyn CgroupFs,
    target: &mut BTreeMap<String, String>,
    cgroup: &str,
    file: &str,
) -> Result<(), String> {
    let value = fs.read_to_string(&cgroup_file(cgroup, file))?;
    target.insert(cgroup.to_string(), value.trim().to_string());
    Ok(())
}

fn validate_owned_child_cgroup(root: &str, cgroup: &str) -> Result<(), String> {
    let cgroup = normalize_cgroup_path(cgroup);
    if is_rejected_target_cgroup(&cgroup) || has_parent_dir(&cgroup) {
        return Err(format!(
            "target cgroup `{cgroup}` is not an owned child cgroup"
        ));
    }
    if !is_strict_child_of(&cgroup, root) {
        return Err(format!(
            "target cgroup `{cgroup}` is outside owned root `{root}`"
        ));
    }

    Ok(())
}

fn validate_owned_rollback_cgroup(cgroup: &str, owned_root: &str) -> Result<(), String> {
    let cgroup = normalize_cgroup_path(cgroup);
    if has_parent_dir(&cgroup) || is_rejected_target_cgroup(&cgroup) {
        return Err(format!(
            "rollback cgroup `{cgroup}` is not an owned child cgroup"
        ));
    }
    if !is_strict_child_of(&cgroup, owned_root) {
        return Err(format!(
            "rollback cgroup `{cgroup}` is outside owned root `{owned_root}`"
        ));
    }

    Ok(())
}

fn is_rejected_target_cgroup(cgroup: &str) -> bool {
    cgroup == "/"
        || cgroup == "/sys/fs/cgroup"
        || cgroup == DEFAULT_AEGISAI_CGROUP_ROOT
        || has_rejected_segment(cgroup)
}

fn is_rejected_original_cgroup(cgroup: &str) -> bool {
    cgroup == "/" || cgroup == "/sys/fs/cgroup" || has_rejected_segment(cgroup)
}

fn has_rejected_segment(path: &str) -> bool {
    Path::new(path).components().any(|component| {
        let Component::Normal(value) = component else {
            return false;
        };
        let value = value.to_string_lossy();
        matches!(
            value.as_ref(),
            "system.slice" | "init.scope" | "machine.slice"
        ) || value.starts_with("kubepods")
            || value.starts_with("docker")
            || value.starts_with("containerd")
            || value.starts_with("podman")
            || value.starts_with("libpod")
            || value.starts_with("lxc")
    })
}

fn normalize_membership_path(raw: &str) -> String {
    let normalized = normalize_cgroup_path(raw);
    if normalized == "/" || normalized.starts_with("/sys/fs/cgroup") {
        normalized
    } else {
        normalize_cgroup_path(&format!("/sys/fs/cgroup{normalized}"))
    }
}

fn normalize_cgroup_path(raw: &str) -> String {
    let trimmed = raw.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        String::new()
    } else if trimmed.starts_with('/') {
        trimmed.to_string()
    } else {
        format!("/{trimmed}")
    }
}

fn is_under_or_equal(path: &str, root: &str) -> bool {
    path == root || is_strict_child_of(path, root)
}

fn is_strict_child_of(path: &str, root: &str) -> bool {
    path.starts_with(&format!("{root}/"))
}

fn has_parent_dir(value: &str) -> bool {
    Path::new(value)
        .components()
        .any(|component| matches!(component, Component::ParentDir))
}

fn cgroup_file(cgroup: &str, file: &str) -> String {
    format!("{}/{}", normalize_cgroup_path(cgroup), file)
}

fn merge_prefixed_execution(
    mut target: BackendExecution,
    prefix: &str,
    source: BackendExecution,
) -> BackendExecution {
    for (key, value) in source.audit_fields {
        target = target.with_field(format!("{prefix}.{key}"), value);
    }
    target
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use super::*;

    #[derive(Clone, Default)]
    struct MemoryCgroupFs {
        state: Rc<RefCell<MemoryCgroupState>>,
    }

    #[derive(Default)]
    struct MemoryCgroupState {
        dirs: BTreeSet<String>,
        files: BTreeMap<String, String>,
        writes: Vec<(String, String)>,
        fail_path: Option<String>,
    }

    impl MemoryCgroupFs {
        fn with_valid_tree() -> Self {
            let fs = Self::default();
            {
                let mut state = fs.state.borrow_mut();
                for dir in [
                    "/sys/fs/cgroup/aegisai.runtime",
                    "/sys/fs/cgroup/aegisai.runtime/protected",
                    "/sys/fs/cgroup/aegisai.runtime/background",
                    "/sys/fs/cgroup/aegisai.runtime/original/app.scope",
                    "/sys/fs/cgroup/aegisai.runtime/original/background.scope",
                ] {
                    state.dirs.insert(dir.to_string());
                }
                for cgroup in [
                    "/sys/fs/cgroup/aegisai.runtime/protected",
                    "/sys/fs/cgroup/aegisai.runtime/background",
                ] {
                    state
                        .files
                        .insert(cgroup_file(cgroup, "cpuset.cpus"), "0-3".to_string());
                    state
                        .files
                        .insert(cgroup_file(cgroup, "cpuset.mems"), "0".to_string());
                    state
                        .files
                        .insert(cgroup_file(cgroup, "cpu.max"), "max 100000".to_string());
                    state
                        .files
                        .insert(cgroup_file(cgroup, "cgroup.procs"), String::new());
                }
                for cgroup in [
                    "/sys/fs/cgroup/aegisai.runtime/original/app.scope",
                    "/sys/fs/cgroup/aegisai.runtime/original/background.scope",
                ] {
                    state
                        .files
                        .insert(cgroup_file(cgroup, "cgroup.procs"), String::new());
                }
            }
            fs
        }

        fn fail_on(&self, path: &str) {
            self.state.borrow_mut().fail_path = Some(path.to_string());
        }

        fn writes(&self) -> Vec<(String, String)> {
            self.state.borrow().writes.clone()
        }
    }

    impl CgroupFs for MemoryCgroupFs {
        fn is_dir(&self, path: &str) -> bool {
            self.state.borrow().dirs.contains(path)
        }

        fn read_to_string(&self, path: &str) -> Result<String, String> {
            self.state
                .borrow()
                .files
                .get(path)
                .cloned()
                .ok_or_else(|| format!("missing `{path}`"))
        }

        fn write_string(&mut self, path: &str, value: &str) -> Result<(), String> {
            if self
                .state
                .borrow()
                .fail_path
                .as_ref()
                .is_some_and(|fail_path| fail_path == path)
            {
                return Err(format!("injected failure for `{path}`"));
            }
            let mut state = self.state.borrow_mut();
            state.writes.push((path.to_string(), value.to_string()));
            state.files.insert(path.to_string(), value.to_string());
            Ok(())
        }
    }

    fn valid_request() -> OwnedCgroupIsolationRequest {
        OwnedCgroupIsolationRequest {
            cgroup_root: "/sys/fs/cgroup/aegisai.runtime".to_string(),
            protected_cgroup: "/sys/fs/cgroup/aegisai.runtime/protected".to_string(),
            background_cgroup: Some("/sys/fs/cgroup/aegisai.runtime/background".to_string()),
            protected_cpuset_cpus: "0".to_string(),
            background_cpuset_cpus: Some("1-3".to_string()),
            cpuset_mems: Some("0".to_string()),
            protected_cpu_max: None,
            background_cpu_max: Some("20000 100000".to_string()),
            protected_targets: vec![CpusetProcessTarget::new(
                42,
                "ollama",
                Some(CpusetProcessClassification::InteractiveAiInference),
            )
            .with_cgroup("/sys/fs/cgroup/aegisai.runtime/original/app.scope")],
            background_targets: vec![CpusetProcessTarget::new(
                84,
                "stress-ng",
                Some(CpusetProcessClassification::BackgroundJob),
            )
            .with_cgroup("/sys/fs/cgroup/aegisai.runtime/original/background.scope")],
        }
    }

    fn valid_guard() -> OwnedCgroupIsolationGuard {
        OwnedCgroupIsolationGuard::aegisai_default([42, 84], true)
            .with_forbidden_pids([999])
            .with_max_processes(4)
    }

    #[test]
    fn owned_cgroup_isolation_applies_and_rolls_back_with_audit() {
        let fs = MemoryCgroupFs::with_valid_tree();
        let mut applier = OwnedCgroupIsolationApplier::with_fs(valid_guard(), fs.clone());

        let result = applier.apply(&valid_request(), 10_000);

        assert_eq!(
            result.execution.audit_fields.get("apply.result"),
            Some(&"ok".to_string())
        );
        assert_eq!(
            result.execution.audit_fields.get("apply.moved_pid_count"),
            Some(&"2".to_string())
        );
        assert_eq!(
            result
                .execution
                .audit_fields
                .get("capture.pid.42.original_cgroup"),
            Some(&"/sys/fs/cgroup/aegisai.runtime/original/app.scope".to_string())
        );

        let lease = result.lease.expect("lease");
        let rollback = applier.rollback(&lease, 10_800);

        assert_eq!(
            rollback.audit_fields.get("rollback.result"),
            Some(&"ok".to_string())
        );
        assert_eq!(
            rollback.audit_fields.get("rollback.success_rate"),
            Some(&"1.000".to_string())
        );

        let writes = fs.writes();
        assert!(writes.contains(&(
            "/sys/fs/cgroup/aegisai.runtime/protected/cpuset.cpus".to_string(),
            "0".to_string()
        )));
        assert!(writes.contains(&(
            "/sys/fs/cgroup/aegisai.runtime/background/cpu.max".to_string(),
            "20000 100000".to_string()
        )));
        assert!(writes.contains(&(
            "/sys/fs/cgroup/aegisai.runtime/protected/cgroup.procs".to_string(),
            "42".to_string()
        )));
        assert!(writes.contains(&(
            "/sys/fs/cgroup/aegisai.runtime/original/app.scope/cgroup.procs".to_string(),
            "42".to_string()
        )));
    }

    #[test]
    fn owned_cgroup_isolation_rejects_target_outside_owned_subtree() {
        let fs = MemoryCgroupFs::with_valid_tree();
        let mut applier = OwnedCgroupIsolationApplier::with_fs(valid_guard(), fs.clone());
        let mut request = valid_request();
        request.protected_cgroup = "/sys/fs/cgroup/system.slice/aegis.service".to_string();

        let result = applier.apply(&request, 10_000);

        assert_eq!(
            result.execution.audit_fields.get("apply.result"),
            Some(&"rejected".to_string())
        );
        assert!(result
            .execution
            .audit_fields
            .get("apply.error")
            .is_some_and(|error| error.contains("not an owned child cgroup")));
        assert!(fs.writes().is_empty());
    }

    #[test]
    fn owned_cgroup_isolation_rejects_original_membership_outside_owned_subtree() {
        let fs = MemoryCgroupFs::with_valid_tree();
        let mut applier = OwnedCgroupIsolationApplier::with_fs(valid_guard(), fs.clone());
        let mut request = valid_request();
        request.protected_targets[0].cgroup =
            Some("/sys/fs/cgroup/user.slice/app.scope".to_string());

        let result = applier.apply(&request, 10_000);

        assert_eq!(
            result.execution.audit_fields.get("apply.result"),
            Some(&"rejected".to_string())
        );
        assert!(result
            .execution
            .audit_fields
            .get("apply.error")
            .is_some_and(|error| error.contains("outside owned root")));
        assert!(fs.writes().is_empty());
    }

    #[test]
    fn owned_cgroup_isolation_rejects_guard_root_outside_aegisai_owned_subtree() {
        let fs = MemoryCgroupFs::with_valid_tree();
        let guard = OwnedCgroupIsolationGuard::new("/sys/fs/cgroup/user.slice", [42, 84], true);
        let mut applier = OwnedCgroupIsolationApplier::with_fs(guard, fs.clone());

        let result = applier.apply(&valid_request(), 10_000);

        assert_eq!(
            result.execution.audit_fields.get("apply.result"),
            Some(&"rejected".to_string())
        );
        assert!(result
            .execution
            .audit_fields
            .get("apply.error")
            .is_some_and(|error| error.contains("outside `/sys/fs/cgroup/aegisai.runtime`")));
        assert!(fs.writes().is_empty());
    }

    #[test]
    fn owned_cgroup_isolation_rejects_traversal_in_owned_root() {
        let fs = MemoryCgroupFs::with_valid_tree();
        let mut applier = OwnedCgroupIsolationApplier::with_fs(valid_guard(), fs.clone());
        let mut request = valid_request();
        request.cgroup_root = "/sys/fs/cgroup/aegisai.runtime/../system.slice".to_string();

        let result = applier.apply(&request, 10_000);

        assert_eq!(
            result.execution.audit_fields.get("apply.result"),
            Some(&"rejected".to_string())
        );
        assert!(result
            .execution
            .audit_fields
            .get("apply.error")
            .is_some_and(|error| error.contains("parent directory traversal")));
        assert!(fs.writes().is_empty());
    }

    #[test]
    fn owned_cgroup_isolation_rejects_container_scope_membership() {
        let fs = MemoryCgroupFs::with_valid_tree();
        let mut applier = OwnedCgroupIsolationApplier::with_fs(valid_guard(), fs.clone());
        let mut request = valid_request();
        request.protected_targets[0].cgroup =
            Some("/sys/fs/cgroup/aegisai.runtime/kubepods.slice/pod.scope".to_string());

        let result = applier.apply(&request, 10_000);

        assert_eq!(
            result.execution.audit_fields.get("apply.result"),
            Some(&"rejected".to_string())
        );
        assert!(result
            .execution
            .audit_fields
            .get("apply.error")
            .is_some_and(|error| error.contains("root/system/container scope")));
        assert!(fs.writes().is_empty());
    }

    #[test]
    fn owned_cgroup_isolation_rejects_mixed_or_unknown_process_sets() {
        let fs = MemoryCgroupFs::with_valid_tree();
        let mut applier = OwnedCgroupIsolationApplier::with_fs(valid_guard(), fs.clone());
        let mut request = valid_request();
        request.background_targets[0].classification =
            Some(CpusetProcessClassification::InteractiveAiInference);

        let result = applier.apply(&request, 10_000);

        assert_eq!(
            result.execution.audit_fields.get("apply.result"),
            Some(&"rejected".to_string())
        );
        assert!(result
            .execution
            .audit_fields
            .get("apply.error")
            .is_some_and(|error| error.contains("not eligible as background work")));
        assert!(fs.writes().is_empty());
    }

    #[test]
    fn owned_cgroup_isolation_rejects_daemon_helper_self_migration() {
        let fs = MemoryCgroupFs::with_valid_tree();
        let guard = valid_guard().with_forbidden_pids([42]);
        let mut applier = OwnedCgroupIsolationApplier::with_fs(guard, fs.clone());

        let result = applier.apply(&valid_request(), 10_000);

        assert_eq!(
            result.execution.audit_fields.get("apply.result"),
            Some(&"rejected".to_string())
        );
        assert!(result
            .execution
            .audit_fields
            .get("apply.error")
            .is_some_and(|error| error.contains("daemon/helper/self pid 42")));
        assert!(fs.writes().is_empty());
    }

    #[test]
    fn owned_cgroup_isolation_write_failure_rolls_back_and_disables() {
        let fs = MemoryCgroupFs::with_valid_tree();
        fs.fail_on("/sys/fs/cgroup/aegisai.runtime/background/cpu.max");
        let mut applier = OwnedCgroupIsolationApplier::with_fs(valid_guard(), fs.clone());

        let result = applier.apply(&valid_request(), 10_000);

        assert_eq!(
            result.execution.audit_fields.get("apply.result"),
            Some(&"error".to_string())
        );
        assert_eq!(
            result.execution.audit_fields.get("apply.failure_disabled"),
            Some(&"true".to_string())
        );
        assert!(result.lease.is_none());
        assert!(applier.disabled());
        assert_eq!(
            result
                .execution
                .audit_fields
                .get("failure_rollback.rollback.attempted_count"),
            Some(&"8".to_string())
        );
        assert_eq!(
            result
                .execution
                .audit_fields
                .get("failure_rollback.rollback.failed_count"),
            Some(&"1".to_string())
        );

        let write_count_after_failure = fs.writes().len();
        let retry = applier.apply(&valid_request(), 10_100);

        assert_eq!(
            retry.execution.audit_fields.get("apply.result"),
            Some(&"disabled".to_string())
        );
        assert_eq!(fs.writes().len(), write_count_after_failure);
    }

    #[test]
    fn owned_cgroup_isolation_rejects_forged_rollback_lease_outside_owned_subtree() {
        let fs = MemoryCgroupFs::with_valid_tree();
        let mut applier = OwnedCgroupIsolationApplier::with_fs(valid_guard(), fs.clone());
        let mut lease = OwnedCgroupIsolationLease::default();
        lease
            .original_memberships
            .insert(42, "/sys/fs/cgroup/user.slice/app.scope".to_string());
        lease
            .original_cpuset_cpus
            .insert("/sys/fs/cgroup/system.slice".to_string(), "0-3".to_string());
        lease
            .original_cpuset_mems
            .insert("/sys/fs/cgroup/system.slice".to_string(), "0".to_string());
        lease.original_cpu_max.insert(
            "/sys/fs/cgroup/system.slice".to_string(),
            "max 100000".to_string(),
        );
        lease
            .touched_cgroups
            .push("/sys/fs/cgroup/system.slice".to_string());

        let rollback = applier.rollback(&lease, 10_900);

        assert_eq!(
            rollback.audit_fields.get("rollback.result"),
            Some(&"error".to_string())
        );
        assert_eq!(
            rollback.audit_fields.get("rollback.failed_count"),
            Some(&"4".to_string())
        );
        assert!(rollback
            .audit_fields
            .get("rollback.0.error")
            .is_some_and(|error| error.contains("outside owned root")));
        assert!(fs.writes().is_empty());
    }
}
