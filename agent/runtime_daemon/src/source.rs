use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::env;
use std::fmt;
use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::thread::JoinHandle;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use ebpf_probe::{
    AttachPoint, Event as ProbeEvent, EventKind as ProbeEventKind, MetricUnit, ProbeConfig,
    ProbeKind, ProbeRegistry,
};
use runtime_orchestrator::RuntimeConfig;
use runtime_orchestrator::SignalKind;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceEvent {
    pub timestamp_ms: u64,
    pub pid: u32,
    pub tid: Option<u32>,
    pub signal: SignalKind,
    pub value: u64,
    pub process_name: Option<String>,
    pub cmdline: Option<String>,
    pub cgroup: Option<String>,
    pub tag_markers: BTreeSet<String>,
    pub parent_pid: Option<u32>,
    pub parent_process_name: Option<String>,
    pub parent_cmdline: Option<String>,
}

impl SourceEvent {
    pub fn new(timestamp_ms: u64, pid: u32, signal: SignalKind, value: u64) -> Self {
        Self {
            timestamp_ms,
            pid,
            tid: None,
            signal,
            value,
            process_name: None,
            cmdline: None,
            cgroup: None,
            tag_markers: BTreeSet::new(),
            parent_pid: None,
            parent_process_name: None,
            parent_cmdline: None,
        }
    }

    pub fn with_tid(mut self, tid: u32) -> Self {
        self.tid = Some(tid);
        self
    }

    pub fn with_process_name(mut self, process_name: impl Into<String>) -> Self {
        self.process_name = Some(process_name.into());
        self
    }

    pub fn with_cmdline(mut self, cmdline: impl Into<String>) -> Self {
        self.cmdline = Some(cmdline.into());
        self
    }

    pub fn with_cgroup(mut self, cgroup: impl Into<String>) -> Self {
        self.cgroup = Some(cgroup.into());
        self
    }

    pub fn with_parent_pid(mut self, parent_pid: u32) -> Self {
        self.parent_pid = Some(parent_pid);
        self
    }

    pub fn with_parent_process_name(mut self, process_name: impl Into<String>) -> Self {
        self.parent_process_name = Some(process_name.into());
        self
    }

    pub fn with_parent_cmdline(mut self, cmdline: impl Into<String>) -> Self {
        self.parent_cmdline = Some(cmdline.into());
        self
    }

    pub fn with_tag_marker(mut self, tag: impl Into<String>) -> Self {
        self.tag_markers.insert(tag.into());
        self
    }

    pub fn with_tag_markers<I, S>(mut self, tags: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.tag_markers = tags.into_iter().map(Into::into).collect();
        self
    }

    pub fn needs_enrichment(&self) -> bool {
        self.process_name.is_none()
            || self.cmdline.is_none()
            || self.cgroup.is_none()
            || self.parent_pid.is_none()
            || self.parent_process_name.is_none()
            || self.parent_cmdline.is_none()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SourceError {
    Unsupported(String),
    InvalidConfig(String),
}

impl fmt::Display for SourceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unsupported(message) => write!(f, "{message}"),
            Self::InvalidConfig(message) => write!(f, "{message}"),
        }
    }
}

impl std::error::Error for SourceError {}

pub trait EventSource {
    fn source_name(&self) -> &str;

    fn next_event(&mut self) -> Result<Option<SourceEvent>, SourceError>;

    fn poll_batch(&mut self, max_batch: usize) -> Result<Vec<SourceEvent>, SourceError> {
        if max_batch == 0 {
            return Err(SourceError::InvalidConfig(
                "event source batch size must be greater than 0".to_string(),
            ));
        }

        let mut events = Vec::with_capacity(max_batch);
        while events.len() < max_batch {
            match self.next_event()? {
                Some(event) => events.push(event),
                None => break,
            }
        }

        Ok(events)
    }
}

pub trait ProbeEventReader {
    fn reader_name(&self) -> &str;

    fn start(
        &mut self,
        plan: &LinuxProbePlan,
        config: &ProbeReaderConfig,
    ) -> Result<ProbeReaderStartup, SourceError>;

    fn next_probe_event(&mut self) -> Result<Option<ProbeEvent>, SourceError>;

    fn poll_probe_events(&mut self, max_events: usize) -> Result<Vec<ProbeEvent>, SourceError> {
        if max_events == 0 {
            return Err(SourceError::InvalidConfig(
                "probe reader batch size must be greater than 0".to_string(),
            ));
        }

        let mut events = Vec::with_capacity(max_events);
        while events.len() < max_events {
            match self.next_probe_event()? {
                Some(event) => events.push(event),
                None => break,
            }
        }

        Ok(events)
    }

    fn stop(&mut self) -> Result<ProbeReaderShutdown, SourceError>;
}

pub trait LinuxProbeDriver {
    fn driver_name(&self) -> &str;

    fn emits_probe_events(&self) -> bool {
        true
    }

    fn no_event_reason(&self) -> Option<String> {
        None
    }

    fn attach_probe(
        &mut self,
        probe: &PlannedProbe,
        config: &ProbeReaderConfig,
    ) -> ProbeAttachmentStatus;

    fn poll_events(
        &mut self,
        max_events: usize,
        timeout_ms: u64,
    ) -> Result<Vec<ProbeEvent>, SourceError>;

    fn stop(&mut self) -> Result<String, SourceError>;
}

pub trait LinuxProbeHost {
    fn host_name(&self) -> &str;

    fn supports_attach_point(&self, attach_point: &AttachPoint) -> Result<(), String>;
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProcfsTargetSelectors {
    process_names: BTreeSet<String>,
    pid_allowlist: BTreeSet<u32>,
}

impl ProcfsTargetSelectors {
    pub fn new<I, S>(process_names: I, pid_allowlist: BTreeSet<u32>) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            process_names: process_names
                .into_iter()
                .map(Into::into)
                .map(|name: String| name.to_ascii_lowercase())
                .collect(),
            pid_allowlist,
        }
    }

    pub fn from_runtime(runtime: &RuntimeConfig) -> Self {
        Self::new(runtime.process_names.clone(), runtime.pid_allowlist.clone())
    }

    fn matches(&self, pid: u32, comm: &str, cmdline: &str) -> bool {
        if self.pid_allowlist.contains(&pid) {
            return true;
        }

        if !self.pid_allowlist.is_empty() && self.process_names.is_empty() {
            return false;
        }

        if self.process_names.is_empty() {
            return true;
        }

        let comm = comm.to_ascii_lowercase();
        let cmdline = cmdline.to_ascii_lowercase();
        self.process_names
            .iter()
            .any(|name| comm == *name || cmdline.contains(name))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProcfsSchedstatSnapshot {
    pub timestamp_ns: u64,
    pub pid: u32,
    pub tid: u32,
    pub comm: String,
    pub run_queue_delay_ns: Option<u64>,
    pub cpu_migrations: Option<u64>,
    pub major_page_faults: Option<u64>,
}

pub trait ProcfsSchedstatSampler {
    fn sampler_name(&self) -> &str;

    fn sample(
        &self,
        selectors: &ProcfsTargetSelectors,
    ) -> Result<Vec<ProcfsSchedstatSnapshot>, SourceError>;
}

#[derive(Default)]
pub struct SystemProcfsSchedstatSampler;

#[cfg(target_os = "linux")]
impl ProcfsSchedstatSampler for SystemProcfsSchedstatSampler {
    fn sampler_name(&self) -> &str {
        "procfs-schedstat"
    }

    fn sample(
        &self,
        selectors: &ProcfsTargetSelectors,
    ) -> Result<Vec<ProcfsSchedstatSnapshot>, SourceError> {
        let mut snapshots = Vec::new();
        let entries = std::fs::read_dir("/proc")
            .map_err(|error| SourceError::Unsupported(format!("failed to read /proc: {error}")))?;

        for entry in entries.flatten() {
            let Some(pid) = entry
                .file_name()
                .to_str()
                .and_then(|value| value.parse::<u32>().ok())
            else {
                continue;
            };

            let root = entry.path();
            let comm = match std::fs::read_to_string(root.join("comm")) {
                Ok(value) => value.trim().to_string(),
                Err(_) => continue,
            };
            let cmdline = std::fs::read(root.join("cmdline"))
                .ok()
                .map(format_cmdline)
                .unwrap_or_default();

            if !selectors.matches(pid, &comm, &cmdline) {
                continue;
            }

            let task_root = root.join("task");
            let mut saw_thread = false;
            if let Ok(tasks) = std::fs::read_dir(&task_root) {
                for task in tasks.flatten() {
                    let Some(tid) = task
                        .file_name()
                        .to_str()
                        .and_then(|value| value.parse::<u32>().ok())
                    else {
                        continue;
                    };
                    let task_path = task.path();
                    let task_comm = std::fs::read_to_string(task_path.join("comm"))
                        .ok()
                        .map(|value| value.trim().to_string())
                        .unwrap_or_else(|| comm.clone());

                    if let Some(snapshot) =
                        read_procfs_schedstat_snapshot(pid, tid, task_comm, &task_path)
                    {
                        saw_thread = true;
                        snapshots.push(snapshot);
                    }
                }
            }

            if !saw_thread {
                if let Some(snapshot) = read_procfs_schedstat_snapshot(pid, pid, comm, &root) {
                    snapshots.push(snapshot);
                }
            }
        }

        Ok(snapshots)
    }
}

#[cfg(target_os = "linux")]
fn read_procfs_schedstat_snapshot(
    pid: u32,
    tid: u32,
    comm: String,
    root: &std::path::Path,
) -> Option<ProcfsSchedstatSnapshot> {
    let run_queue_delay_ns = std::fs::read_to_string(root.join("schedstat"))
        .ok()
        .and_then(|raw| parse_schedstat_run_delay_ns(&raw));
    let cpu_migrations = std::fs::read_to_string(root.join("sched"))
        .ok()
        .and_then(|raw| parse_sched_value(&raw, "se.nr_migrations"));
    let major_page_faults = std::fs::read_to_string(root.join("stat"))
        .ok()
        .and_then(|raw| parse_stat_major_page_faults(&raw));

    if run_queue_delay_ns.is_none() && cpu_migrations.is_none() && major_page_faults.is_none() {
        return None;
    }

    Some(ProcfsSchedstatSnapshot {
        timestamp_ns: now_ns(),
        pid,
        tid,
        comm,
        run_queue_delay_ns,
        cpu_migrations,
        major_page_faults,
    })
}

#[cfg(not(target_os = "linux"))]
impl ProcfsSchedstatSampler for SystemProcfsSchedstatSampler {
    fn sampler_name(&self) -> &str {
        "procfs-unavailable"
    }

    fn sample(
        &self,
        _selectors: &ProcfsTargetSelectors,
    ) -> Result<Vec<ProcfsSchedstatSnapshot>, SourceError> {
        Err(SourceError::Unsupported(
            "procfs schedstat sampling is only available on Linux".to_string(),
        ))
    }
}

pub struct ProcfsSchedstatProbeDriver<S> {
    sampler: S,
    selectors: ProcfsTargetSelectors,
    attached_run_queue_delay: bool,
    attached_cpu_migration: bool,
    attached_major_page_fault: bool,
    previous_run_delay_ns: BTreeMap<(u32, u32), u64>,
    previous_cpu_migrations: BTreeMap<(u32, u32), u64>,
    previous_major_page_faults: BTreeMap<(u32, u32), u64>,
}

impl ProcfsSchedstatProbeDriver<SystemProcfsSchedstatSampler> {
    pub fn from_runtime(runtime: &RuntimeConfig) -> Self {
        Self::new(
            ProcfsTargetSelectors::from_runtime(runtime),
            SystemProcfsSchedstatSampler,
        )
    }
}

impl<S> ProcfsSchedstatProbeDriver<S> {
    pub fn new(selectors: ProcfsTargetSelectors, sampler: S) -> Self {
        Self {
            sampler,
            selectors,
            attached_run_queue_delay: false,
            attached_cpu_migration: false,
            attached_major_page_fault: false,
            previous_run_delay_ns: BTreeMap::new(),
            previous_cpu_migrations: BTreeMap::new(),
            previous_major_page_faults: BTreeMap::new(),
        }
    }
}

impl<S> LinuxProbeDriver for ProcfsSchedstatProbeDriver<S>
where
    S: ProcfsSchedstatSampler,
{
    fn driver_name(&self) -> &str {
        "procfs-schedstat-driver"
    }

    fn attach_probe(
        &mut self,
        probe: &PlannedProbe,
        _config: &ProbeReaderConfig,
    ) -> ProbeAttachmentStatus {
        if let Err(error) = probe.config.validate() {
            return ProbeAttachmentStatus::Failed(error.to_string());
        }

        let supports_all_signals = probe.required_signals.iter().all(|signal| {
            matches!(
                (probe.kind, signal),
                (ProbeKind::Sched, SignalKind::RunQueueDelay)
                    | (ProbeKind::Sched, SignalKind::CpuMigration)
                    | (ProbeKind::Fault, SignalKind::MajorPageFault)
            )
        });

        if !supports_all_signals || probe.required_signals.is_empty() {
            return ProbeAttachmentStatus::Failed(
                "procfs driver supports run_queue_delay/cpu_migration via sched_probe and major_page_fault via fault_probe".to_string(),
            );
        }

        for signal in &probe.required_signals {
            match signal {
                SignalKind::RunQueueDelay => self.attached_run_queue_delay = true,
                SignalKind::CpuMigration => self.attached_cpu_migration = true,
                SignalKind::MajorPageFault => self.attached_major_page_fault = true,
                _ => {}
            }
        }

        ProbeAttachmentStatus::Attached
    }

    fn poll_events(
        &mut self,
        max_events: usize,
        timeout_ms: u64,
    ) -> Result<Vec<ProbeEvent>, SourceError> {
        if max_events == 0
            || !(self.attached_run_queue_delay
                || self.attached_cpu_migration
                || self.attached_major_page_fault)
        {
            return Ok(Vec::new());
        }

        let events = self.collect_delta_events(max_events)?;
        if !events.is_empty() || timeout_ms == 0 {
            return Ok(events);
        }

        std::thread::sleep(std::time::Duration::from_millis(timeout_ms));
        self.collect_delta_events(max_events)
    }

    fn stop(&mut self) -> Result<String, SourceError> {
        Ok(format!(
            "procfs driver stopped after tracking {} schedstat target(s), {} migration target(s), and {} fault target(s) with {}",
            self.previous_run_delay_ns.len(),
            self.previous_cpu_migrations.len(),
            self.previous_major_page_faults.len(),
            self.sampler.sampler_name()
        ))
    }
}

impl<S> ProcfsSchedstatProbeDriver<S>
where
    S: ProcfsSchedstatSampler,
{
    fn collect_delta_events(&mut self, max_events: usize) -> Result<Vec<ProbeEvent>, SourceError> {
        let mut events = Vec::new();
        for snapshot in self.sampler.sample(&self.selectors)? {
            if events.len() >= max_events {
                break;
            }

            if self.attached_run_queue_delay {
                if let Some(event) = delta_probe_event(
                    &mut self.previous_run_delay_ns,
                    &snapshot,
                    snapshot.run_queue_delay_ns,
                    ProbeKind::Sched,
                    ProbeEventKind::RunQueueDelay,
                    ebpf_probe::EventMetric::duration_ns,
                ) {
                    events.push(event);
                }
            }

            if self.attached_cpu_migration && events.len() < max_events {
                if let Some(event) = delta_probe_event(
                    &mut self.previous_cpu_migrations,
                    &snapshot,
                    snapshot.cpu_migrations,
                    ProbeKind::Sched,
                    ProbeEventKind::CpuMigration,
                    ebpf_probe::EventMetric::count,
                ) {
                    events.push(event);
                }
            }

            if self.attached_major_page_fault && events.len() < max_events {
                if let Some(event) = delta_probe_event(
                    &mut self.previous_major_page_faults,
                    &snapshot,
                    snapshot.major_page_faults,
                    ProbeKind::Fault,
                    ProbeEventKind::MajorPageFault,
                    ebpf_probe::EventMetric::count,
                ) {
                    events.push(event);
                }
            }
        }

        Ok(events)
    }
}

pub struct RealLinuxProbeDriver<P, S> {
    procfs: ProcfsSchedstatProbeDriver<S>,
    bpftrace: BpfTraceProbeDriver<P>,
    attached_procfs: bool,
    attached_bpftrace: bool,
}

impl RealLinuxProbeDriver<SystemBpfTracePipe, SystemProcfsSchedstatSampler> {
    pub fn from_runtime(runtime: &RuntimeConfig) -> Self {
        Self::new(
            ProcfsSchedstatProbeDriver::from_runtime(runtime),
            BpfTraceProbeDriver::from_runtime(runtime),
        )
    }
}

impl<P, S> RealLinuxProbeDriver<P, S> {
    pub fn new(procfs: ProcfsSchedstatProbeDriver<S>, bpftrace: BpfTraceProbeDriver<P>) -> Self {
        Self {
            procfs,
            bpftrace,
            attached_procfs: false,
            attached_bpftrace: false,
        }
    }
}

impl<P, S> LinuxProbeDriver for RealLinuxProbeDriver<P, S>
where
    P: BpfTracePipe,
    S: ProcfsSchedstatSampler,
{
    fn driver_name(&self) -> &str {
        "real-linux-probe-driver"
    }

    fn attach_probe(
        &mut self,
        probe: &PlannedProbe,
        config: &ProbeReaderConfig,
    ) -> ProbeAttachmentStatus {
        let status = match probe.kind {
            ProbeKind::Sched | ProbeKind::Fault => self.procfs.attach_probe(probe, config),
            ProbeKind::OffCpu | ProbeKind::Io => self.bpftrace.attach_probe(probe, config),
        };

        if status == ProbeAttachmentStatus::Attached {
            match probe.kind {
                ProbeKind::Sched | ProbeKind::Fault => self.attached_procfs = true,
                ProbeKind::OffCpu | ProbeKind::Io => self.attached_bpftrace = true,
            }
        }

        status
    }

    fn poll_events(
        &mut self,
        max_events: usize,
        timeout_ms: u64,
    ) -> Result<Vec<ProbeEvent>, SourceError> {
        if max_events == 0 {
            return Ok(Vec::new());
        }

        let mut events = self.bpftrace.poll_events(max_events, 0)?;
        if events.len() < max_events {
            events.extend(
                self.procfs
                    .poll_events(max_events.saturating_sub(events.len()), 0)?,
            );
        }

        if !events.is_empty() || timeout_ms == 0 {
            return Ok(events);
        }

        if self.attached_bpftrace {
            events.extend(self.bpftrace.poll_events(max_events, timeout_ms)?);
            if events.len() < max_events && self.attached_procfs {
                events.extend(
                    self.procfs
                        .poll_events(max_events.saturating_sub(events.len()), 0)?,
                );
            }
        } else if self.attached_procfs {
            events.extend(self.procfs.poll_events(max_events, timeout_ms)?);
        } else {
            std::thread::sleep(Duration::from_millis(timeout_ms));
        }

        Ok(events)
    }

    fn stop(&mut self) -> Result<String, SourceError> {
        let bpftrace_stop = self.bpftrace.stop()?;
        let procfs_stop = self.procfs.stop()?;
        Ok(format!("{bpftrace_stop}; {procfs_stop}"))
    }
}

pub trait BpfTracePipe {
    fn pipe_name(&self) -> &str;

    fn check_available(&self) -> Result<(), String>;

    fn start(&mut self, program: &str) -> Result<(), SourceError>;

    fn read_lines(&mut self, max_lines: usize, timeout_ms: u64)
        -> Result<Vec<String>, SourceError>;

    fn stop(&mut self) -> Result<String, SourceError>;
}

pub struct BpfTraceProbeDriver<P> {
    pipe: P,
    selectors: ProcfsTargetSelectors,
    attached_offcpu: bool,
    attached_io: bool,
    started: bool,
}

impl BpfTraceProbeDriver<SystemBpfTracePipe> {
    pub fn from_runtime(runtime: &RuntimeConfig) -> Self {
        Self::new(
            ProcfsTargetSelectors::from_runtime(runtime),
            SystemBpfTracePipe::default(),
        )
    }
}

impl<P> BpfTraceProbeDriver<P> {
    pub fn new(selectors: ProcfsTargetSelectors, pipe: P) -> Self {
        Self {
            pipe,
            selectors,
            attached_offcpu: false,
            attached_io: false,
            started: false,
        }
    }
}

impl<P> LinuxProbeDriver for BpfTraceProbeDriver<P>
where
    P: BpfTracePipe,
{
    fn driver_name(&self) -> &str {
        self.pipe.pipe_name()
    }

    fn attach_probe(
        &mut self,
        probe: &PlannedProbe,
        _config: &ProbeReaderConfig,
    ) -> ProbeAttachmentStatus {
        if let Err(error) = probe.config.validate() {
            return ProbeAttachmentStatus::Failed(error.to_string());
        }

        let supports_all_signals = probe.required_signals.iter().all(|signal| {
            matches!(
                (probe.kind, signal),
                (ProbeKind::OffCpu, SignalKind::OffCpuTime)
                    | (ProbeKind::Io, SignalKind::IoLatency)
            )
        });

        if !supports_all_signals || probe.required_signals.is_empty() {
            return ProbeAttachmentStatus::Failed(
                "bpftrace driver supports offcpu_time via offcpu_probe and io_latency via io_probe"
                    .to_string(),
            );
        }

        if let Err(reason) = self.pipe.check_available() {
            return ProbeAttachmentStatus::Failed(reason);
        }

        for signal in &probe.required_signals {
            match signal {
                SignalKind::OffCpuTime => self.attached_offcpu = true,
                SignalKind::IoLatency => self.attached_io = true,
                _ => {}
            }
        }

        ProbeAttachmentStatus::Attached
    }

    fn poll_events(
        &mut self,
        max_events: usize,
        timeout_ms: u64,
    ) -> Result<Vec<ProbeEvent>, SourceError> {
        if max_events == 0 || !(self.attached_offcpu || self.attached_io) {
            return Ok(Vec::new());
        }

        self.ensure_started()?;

        let max_lines = max_events.saturating_mul(8).max(max_events);
        let mut events = Vec::new();
        for line in self.pipe.read_lines(max_lines, timeout_ms)? {
            let Some(event) = parse_bpftrace_probe_line(&line)? else {
                continue;
            };

            if self.accepts_event(&event) {
                events.push(event);
                if events.len() >= max_events {
                    break;
                }
            }
        }

        Ok(events)
    }

    fn stop(&mut self) -> Result<String, SourceError> {
        self.started = false;
        self.pipe.stop()
    }
}

impl<P> BpfTraceProbeDriver<P>
where
    P: BpfTracePipe,
{
    fn ensure_started(&mut self) -> Result<(), SourceError> {
        if self.started {
            return Ok(());
        }

        let program = bpftrace_program(&self.selectors, self.attached_offcpu, self.attached_io);
        self.pipe.start(&program)?;
        self.started = true;
        Ok(())
    }

    fn accepts_event(&self, event: &ProbeEvent) -> bool {
        matches!(
            event.kind,
            ProbeEventKind::OffCpuDuration if self.attached_offcpu
        ) || matches!(
            event.kind,
            ProbeEventKind::BlockIoLatency if self.attached_io
        )
    }
}

pub struct SystemBpfTracePipe {
    command: String,
    child: Option<Child>,
    receiver: Option<Receiver<String>>,
    reader_threads: Vec<JoinHandle<()>>,
}

impl Default for SystemBpfTracePipe {
    fn default() -> Self {
        Self {
            command: env::var("AEGISAI_BPFTRACE").unwrap_or_else(|_| "bpftrace".to_string()),
            child: None,
            receiver: None,
            reader_threads: Vec::new(),
        }
    }
}

impl BpfTracePipe for SystemBpfTracePipe {
    fn pipe_name(&self) -> &str {
        "bpftrace-ebpf-driver"
    }

    fn check_available(&self) -> Result<(), String> {
        if !command_exists(&self.command) {
            return Err(format!(
                "`{}` is not available in PATH; install bpftrace to stream real offcpu/io eBPF events",
                self.command
            ));
        }

        bpftrace_permission_check()?;
        Ok(())
    }

    fn start(&mut self, program: &str) -> Result<(), SourceError> {
        if self.child.is_some() {
            return Ok(());
        }

        let mut child = Command::new(&self.command)
            .arg("-q")
            .arg("-e")
            .arg(program)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| {
                SourceError::Unsupported(format!(
                    "failed to start `{}` for real eBPF probe ingestion: {error}",
                    self.command
                ))
            })?;

        let stdout = child.stdout.take().ok_or_else(|| {
            SourceError::Unsupported("failed to capture bpftrace stdout".to_string())
        })?;
        let stderr = child.stderr.take().ok_or_else(|| {
            SourceError::Unsupported("failed to capture bpftrace stderr".to_string())
        })?;
        let (sender, receiver) = mpsc::channel();
        self.reader_threads
            .push(spawn_pipe_reader(stdout, sender.clone()));
        self.reader_threads.push(spawn_pipe_reader(stderr, sender));
        self.receiver = Some(receiver);
        self.child = Some(child);
        Ok(())
    }

    fn read_lines(
        &mut self,
        max_lines: usize,
        timeout_ms: u64,
    ) -> Result<Vec<String>, SourceError> {
        if max_lines == 0 {
            return Ok(Vec::new());
        }

        if let Some(child) = self.child.as_mut() {
            if let Some(status) = child.try_wait().map_err(|error| {
                SourceError::Unsupported(format!("failed to check bpftrace status: {error}"))
            })? {
                return Err(SourceError::Unsupported(format!(
                    "bpftrace exited before probe events were available: {status}"
                )));
            }
        }

        let Some(receiver) = self.receiver.as_ref() else {
            return Err(SourceError::InvalidConfig(
                "bpftrace pipe must be started before reading events".to_string(),
            ));
        };

        let mut lines = Vec::new();
        let deadline = std::time::Instant::now() + Duration::from_millis(timeout_ms);
        while lines.len() < max_lines {
            if timeout_ms == 0 || !lines.is_empty() {
                match receiver.try_recv() {
                    Ok(line) => lines.push(line),
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => break,
                }
            } else {
                let now = std::time::Instant::now();
                if now >= deadline {
                    break;
                }
                match receiver.recv_timeout(deadline.saturating_duration_since(now)) {
                    Ok(line) => lines.push(line),
                    Err(mpsc::RecvTimeoutError::Timeout) => break,
                    Err(mpsc::RecvTimeoutError::Disconnected) => break,
                }
            }
        }

        Ok(lines)
    }

    fn stop(&mut self) -> Result<String, SourceError> {
        let mut stop_reason = "bpftrace eBPF driver stopped without a running child".to_string();
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            match child.wait() {
                Ok(status) => {
                    stop_reason = format!("bpftrace eBPF driver stopped with status {status}");
                }
                Err(error) => {
                    stop_reason = format!("bpftrace eBPF driver stop wait failed: {error}");
                }
            }
        }

        self.receiver = None;
        for thread in self.reader_threads.drain(..) {
            let _ = thread.join();
        }
        Ok(stop_reason)
    }
}

impl Drop for SystemBpfTracePipe {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

#[cfg(test)]
struct FakeBpfTracePipe {
    lines: VecDeque<Vec<String>>,
    started_program: Option<String>,
    available: Result<(), String>,
    stopped: bool,
}

#[cfg(test)]
impl FakeBpfTracePipe {
    fn new(lines: Vec<Vec<&str>>) -> Self {
        Self {
            lines: lines
                .into_iter()
                .map(|batch| batch.into_iter().map(str::to_string).collect())
                .collect(),
            started_program: None,
            available: Ok(()),
            stopped: false,
        }
    }

    fn unavailable(reason: impl Into<String>) -> Self {
        Self {
            lines: VecDeque::new(),
            started_program: None,
            available: Err(reason.into()),
            stopped: false,
        }
    }
}

#[cfg(test)]
impl BpfTracePipe for FakeBpfTracePipe {
    fn pipe_name(&self) -> &str {
        "fake-bpftrace"
    }

    fn check_available(&self) -> Result<(), String> {
        self.available.clone()
    }

    fn start(&mut self, program: &str) -> Result<(), SourceError> {
        self.started_program = Some(program.to_string());
        Ok(())
    }

    fn read_lines(
        &mut self,
        max_lines: usize,
        _timeout_ms: u64,
    ) -> Result<Vec<String>, SourceError> {
        let mut lines = self.lines.pop_front().unwrap_or_default();
        lines.truncate(max_lines);
        Ok(lines)
    }

    fn stop(&mut self) -> Result<String, SourceError> {
        self.stopped = true;
        Ok("fake bpftrace stopped".to_string())
    }
}

fn spawn_pipe_reader<R>(reader: R, sender: mpsc::Sender<String>) -> JoinHandle<()>
where
    R: std::io::Read + Send + 'static,
{
    std::thread::spawn(move || {
        let reader = BufReader::new(reader);
        for line in reader.lines().map_while(Result::ok) {
            if sender.send(line).is_err() {
                break;
            }
        }
    })
}

fn parse_bpftrace_probe_line(raw: &str) -> Result<Option<ProbeEvent>, SourceError> {
    let raw = raw.trim();
    let Some(rest) = raw.strip_prefix("aegisai_probe ") else {
        return Ok(None);
    };

    let fields = rest
        .split_whitespace()
        .filter_map(|part| part.split_once('='))
        .collect::<BTreeMap<_, _>>();

    let signal = required_bpftrace_field(&fields, "signal")?;
    let timestamp_ns = parse_bpftrace_u64(&fields, "ts_ns")?;
    let pid = parse_bpftrace_u32(&fields, "pid")?;
    let tid = parse_bpftrace_u32(&fields, "tid")?;
    let comm = required_bpftrace_field(&fields, "comm")?;
    let value_ns = parse_bpftrace_u64(&fields, "value_ns")?;

    let (probe, kind) = match signal {
        "offcpu_time" => (ProbeKind::OffCpu, ProbeEventKind::OffCpuDuration),
        "io_latency" => (ProbeKind::Io, ProbeEventKind::BlockIoLatency),
        other => {
            return Err(SourceError::InvalidConfig(format!(
                "unsupported bpftrace probe signal `{other}`"
            )));
        }
    };

    let event = ProbeEvent::new(
        timestamp_ns,
        probe,
        kind,
        ebpf_probe::EventTarget::new(pid, tid, comm),
        ebpf_probe::EventMetric::duration_ns(value_ns),
    );
    event.validate().map_err(|error| {
        SourceError::InvalidConfig(format!("invalid bpftrace probe event `{raw}`: {error}"))
    })?;
    Ok(Some(event))
}

fn required_bpftrace_field<'a>(
    fields: &BTreeMap<&'a str, &'a str>,
    name: &str,
) -> Result<&'a str, SourceError> {
    fields.get(name).copied().ok_or_else(|| {
        SourceError::InvalidConfig(format!("bpftrace probe event is missing `{name}`"))
    })
}

fn parse_bpftrace_u64(fields: &BTreeMap<&str, &str>, name: &str) -> Result<u64, SourceError> {
    required_bpftrace_field(fields, name)?
        .parse::<u64>()
        .map_err(|_| SourceError::InvalidConfig(format!("invalid bpftrace `{name}` value")))
}

fn parse_bpftrace_u32(fields: &BTreeMap<&str, &str>, name: &str) -> Result<u32, SourceError> {
    required_bpftrace_field(fields, name)?
        .parse::<u32>()
        .map_err(|_| SourceError::InvalidConfig(format!("invalid bpftrace `{name}` value")))
}

fn bpftrace_program(
    selectors: &ProcfsTargetSelectors,
    include_offcpu: bool,
    include_io: bool,
) -> String {
    let offcpu_predicate = bpftrace_target_predicate(selectors, "args->prev_pid", "comm");
    let io_predicate = bpftrace_target_predicate(selectors, "tid", "comm");
    let mut sections = Vec::new();

    if include_offcpu {
        sections.push(format!(
            r#"tracepoint:sched:sched_switch
/args->prev_state != 0 && ({offcpu_predicate})/
{{
  @aegisai_offcpu_ts[args->prev_pid] = nsecs;
  @aegisai_offcpu_pid[args->prev_pid] = pid;
}}

tracepoint:sched:sched_switch
/@aegisai_offcpu_ts[args->next_pid]/
{{
  $delta = nsecs - @aegisai_offcpu_ts[args->next_pid];
  if ($delta > 0) {{
    printf("aegisai_probe signal=offcpu_time ts_ns=%llu pid=%u tid=%u comm=%s value_ns=%llu\n",
      nsecs, @aegisai_offcpu_pid[args->next_pid], args->next_pid, str(args->next_comm), $delta);
  }}
  delete(@aegisai_offcpu_ts[args->next_pid]);
  delete(@aegisai_offcpu_pid[args->next_pid]);
}}"#
        ));
    }

    if include_io {
        sections.push(format!(
            r#"tracepoint:block:block_rq_issue
/({io_predicate})/
{{
  @aegisai_io_ts[args->dev, args->sector] = nsecs;
  @aegisai_io_pid[args->dev, args->sector] = pid;
  @aegisai_io_tid[args->dev, args->sector] = tid;
  @aegisai_io_comm[args->dev, args->sector] = comm;
}}

tracepoint:block:block_rq_complete
/@aegisai_io_ts[args->dev, args->sector]/
{{
  $delta = nsecs - @aegisai_io_ts[args->dev, args->sector];
  if ($delta > 0) {{
    printf("aegisai_probe signal=io_latency ts_ns=%llu pid=%u tid=%u comm=%s value_ns=%llu\n",
      nsecs, @aegisai_io_pid[args->dev, args->sector], @aegisai_io_tid[args->dev, args->sector], @aegisai_io_comm[args->dev, args->sector], $delta);
  }}
  delete(@aegisai_io_ts[args->dev, args->sector]);
  delete(@aegisai_io_pid[args->dev, args->sector]);
  delete(@aegisai_io_tid[args->dev, args->sector]);
  delete(@aegisai_io_comm[args->dev, args->sector]);
}}"#
        ));
    }

    sections.join("\n\n")
}

fn bpftrace_target_predicate(
    selectors: &ProcfsTargetSelectors,
    tid_expr: &str,
    comm_expr: &str,
) -> String {
    let mut predicates = Vec::new();

    for pid in &selectors.pid_allowlist {
        predicates.push(format!("pid == {pid}"));
        predicates.push(format!("{tid_expr} == {pid}"));
    }

    for process_name in &selectors.process_names {
        predicates.push(format!(
            "{comm_expr} == \"{}\"",
            escape_bpftrace_string(process_name)
        ));
    }

    if predicates.is_empty() {
        "1".to_string()
    } else {
        predicates.join(" || ")
    }
}

fn escape_bpftrace_string(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

fn command_exists(command: &str) -> bool {
    if command.contains('/') {
        return std::path::Path::new(command).is_file();
    }

    let Some(paths) = env::var_os("PATH") else {
        return false;
    };

    env::split_paths(&paths).any(|path| path.join(command).is_file())
}

#[cfg(target_os = "linux")]
fn current_effective_uid() -> Option<u32> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    let uid_line = status.lines().find(|line| line.starts_with("Uid:"))?;
    uid_line.split_whitespace().nth(2)?.parse::<u32>().ok()
}

#[cfg(target_os = "linux")]
fn bpftrace_permission_check() -> Result<(), String> {
    if current_effective_uid() == Some(0) {
        return Ok(());
    }

    Err(
        "bpftrace eBPF probes require root on this host; rerun as root or use --allow-partial-probes for procfs-only fallback"
            .to_string(),
    )
}

#[cfg(not(target_os = "linux"))]
fn bpftrace_permission_check() -> Result<(), String> {
    Err("bpftrace eBPF probes are only available on Linux".to_string())
}

fn delta_probe_event(
    previous_values: &mut BTreeMap<(u32, u32), u64>,
    snapshot: &ProcfsSchedstatSnapshot,
    current_value: Option<u64>,
    probe: ProbeKind,
    kind: ProbeEventKind,
    metric_from_delta: fn(u64) -> ebpf_probe::EventMetric,
) -> Option<ProbeEvent> {
    let current_value = current_value?;

    let key = (snapshot.pid, snapshot.tid);
    let previous = previous_values.insert(key, current_value);
    let previous_value = previous?;

    let delta = current_value.saturating_sub(previous_value);
    if delta == 0 {
        return None;
    }

    Some(ProbeEvent::new(
        snapshot.timestamp_ns,
        probe,
        kind,
        ebpf_probe::EventTarget::new(snapshot.pid, snapshot.tid, snapshot.comm.clone()),
        metric_from_delta(delta),
    ))
}

pub struct PreflightLinuxProbeDriver<H> {
    host: H,
    attached_probes: usize,
    failed_probes: usize,
}

impl<H> PreflightLinuxProbeDriver<H> {
    pub fn new(host: H) -> Self {
        Self {
            host,
            attached_probes: 0,
            failed_probes: 0,
        }
    }
}

impl PreflightLinuxProbeDriver<SystemLinuxProbeHost> {
    pub fn system() -> Self {
        Self::new(SystemLinuxProbeHost)
    }
}

impl<H> LinuxProbeDriver for PreflightLinuxProbeDriver<H>
where
    H: LinuxProbeHost,
{
    fn driver_name(&self) -> &str {
        "preflight-probe-driver"
    }

    fn emits_probe_events(&self) -> bool {
        false
    }

    fn no_event_reason(&self) -> Option<String> {
        Some(format!(
            "preflight driver audits attach prerequisites on {} but does not load eBPF programs or read ring buffers",
            self.host.host_name()
        ))
    }

    fn attach_probe(
        &mut self,
        probe: &PlannedProbe,
        _config: &ProbeReaderConfig,
    ) -> ProbeAttachmentStatus {
        if let Err(error) = probe.config.validate() {
            self.failed_probes = self.failed_probes.saturating_add(1);
            return ProbeAttachmentStatus::Failed(error.to_string());
        }

        for attach_point in &probe.attach_points {
            if let Err(reason) = self.host.supports_attach_point(attach_point) {
                self.failed_probes = self.failed_probes.saturating_add(1);
                return ProbeAttachmentStatus::Failed(reason);
            }
        }

        self.attached_probes = self.attached_probes.saturating_add(1);
        ProbeAttachmentStatus::Attached
    }

    fn poll_events(
        &mut self,
        _max_events: usize,
        _timeout_ms: u64,
    ) -> Result<Vec<ProbeEvent>, SourceError> {
        Ok(Vec::new())
    }

    fn stop(&mut self) -> Result<String, SourceError> {
        Ok(format!(
            "preflight driver stopped on {} after attaching {} probe(s) and rejecting {} probe(s)",
            self.host.host_name(),
            self.attached_probes,
            self.failed_probes
        ))
    }
}

#[cfg(target_os = "linux")]
#[derive(Default)]
pub struct SystemLinuxProbeHost;

#[cfg(target_os = "linux")]
impl LinuxProbeHost for SystemLinuxProbeHost {
    fn host_name(&self) -> &str {
        "linux"
    }

    fn supports_attach_point(&self, attach_point: &AttachPoint) -> Result<(), String> {
        let tracefs = detect_tracefs_root().ok_or_else(|| {
            "tracefs is not mounted under /sys/kernel/tracing or /sys/kernel/debug/tracing"
                .to_string()
        })?;

        match attach_point {
            AttachPoint::TracePoint { category, name } => {
                let path = tracefs.join("events").join(category).join(name).join("id");
                if path.is_file() {
                    Ok(())
                } else {
                    Err(format!("tracepoint {category}/{name} is not available"))
                }
            }
            AttachPoint::KProbe { function } | AttachPoint::KRetProbe { function } => {
                let kprobe_events = tracefs.join("kprobe_events");
                if !kprobe_events.exists() {
                    return Err("kprobe_events is not available under tracefs".to_string());
                }

                let kallsyms = std::fs::read_to_string("/proc/kallsyms")
                    .map_err(|error| format!("failed to read /proc/kallsyms: {error}"))?;
                let exists = kallsyms
                    .lines()
                    .filter_map(|line| line.split_whitespace().nth(2))
                    .any(|symbol| symbol == *function);
                if exists {
                    Ok(())
                } else {
                    Err(format!("kernel symbol `{function}` is not available"))
                }
            }
            AttachPoint::RawTracePoint { name } => {
                let path = tracefs.join("events").join(name).join("id");
                if path.is_file() {
                    Ok(())
                } else {
                    Err(format!("raw tracepoint `{name}` is not available"))
                }
            }
        }
    }
}

#[cfg(target_os = "linux")]
fn detect_tracefs_root() -> Option<std::path::PathBuf> {
    ["/sys/kernel/tracing", "/sys/kernel/debug/tracing"]
        .into_iter()
        .map(std::path::Path::new)
        .find(|path| path.join("events").is_dir())
        .map(std::path::Path::to_path_buf)
}

#[cfg(not(target_os = "linux"))]
#[derive(Default)]
pub struct SystemLinuxProbeHost;

#[cfg(not(target_os = "linux"))]
impl LinuxProbeHost for SystemLinuxProbeHost {
    fn host_name(&self) -> &str {
        "non-linux"
    }

    fn supports_attach_point(&self, _attach_point: &AttachPoint) -> Result<(), String> {
        Err("linux probe preflight checks are only available on Linux".to_string())
    }
}

pub struct DriverBackedProbeEventReader<D> {
    driver: D,
    buffered_events: VecDeque<ProbeEvent>,
    started: bool,
    max_buffered_events: usize,
    poll_timeout_ms: u64,
    emitted_events: u64,
}

impl<D> DriverBackedProbeEventReader<D> {
    pub fn new(driver: D) -> Self {
        Self {
            driver,
            buffered_events: VecDeque::new(),
            started: false,
            max_buffered_events: 0,
            poll_timeout_ms: 0,
            emitted_events: 0,
        }
    }
}

impl<D> ProbeEventReader for DriverBackedProbeEventReader<D>
where
    D: LinuxProbeDriver,
{
    fn reader_name(&self) -> &str {
        self.driver.driver_name()
    }

    fn start(
        &mut self,
        plan: &LinuxProbePlan,
        config: &ProbeReaderConfig,
    ) -> Result<ProbeReaderStartup, SourceError> {
        self.started = true;
        self.max_buffered_events = config.max_buffered_events;
        self.poll_timeout_ms = config.poll_timeout_ms;
        let reader_name = self.driver.driver_name().to_string();

        let mut startup = ProbeReaderStartup::from_plan(reader_name, plan, config, |probe| {
            self.driver.attach_probe(probe, config)
        });
        startup.emits_probe_events = self.driver.emits_probe_events();
        startup.no_event_reason = self.driver.no_event_reason();
        Ok(startup)
    }

    fn next_probe_event(&mut self) -> Result<Option<ProbeEvent>, SourceError> {
        if !self.started {
            return Err(SourceError::InvalidConfig(
                "probe reader must be started before polling events".to_string(),
            ));
        }

        if self.buffered_events.is_empty() {
            let events = self
                .driver
                .poll_events(self.max_buffered_events, self.poll_timeout_ms)?;
            self.buffered_events.extend(events);
        }

        let event = self.buffered_events.pop_front();
        if event.is_some() {
            self.emitted_events = self.emitted_events.saturating_add(1);
        }
        Ok(event)
    }

    fn poll_probe_events(&mut self, max_events: usize) -> Result<Vec<ProbeEvent>, SourceError> {
        if max_events == 0 {
            return Err(SourceError::InvalidConfig(
                "probe reader batch size must be greater than 0".to_string(),
            ));
        }
        if !self.started {
            return Err(SourceError::InvalidConfig(
                "probe reader must be started before polling events".to_string(),
            ));
        }

        if self.buffered_events.is_empty() {
            let events = self
                .driver
                .poll_events(self.max_buffered_events, self.poll_timeout_ms)?;
            self.buffered_events.extend(events);
        }

        let mut events = Vec::with_capacity(max_events.min(self.buffered_events.len()));
        while events.len() < max_events {
            let Some(event) = self.buffered_events.pop_front() else {
                break;
            };
            self.emitted_events = self.emitted_events.saturating_add(1);
            events.push(event);
        }

        Ok(events)
    }

    fn stop(&mut self) -> Result<ProbeReaderShutdown, SourceError> {
        self.started = false;
        Ok(ProbeReaderShutdown {
            reader_name: self.driver.driver_name().to_string(),
            emitted_events: self.emitted_events,
            stop_reason: self.driver.stop()?,
        })
    }
}

#[derive(Default)]
pub struct UnavailableLinuxProbeDriver;

impl LinuxProbeDriver for UnavailableLinuxProbeDriver {
    fn driver_name(&self) -> &str {
        "unavailable-probe-driver"
    }

    fn emits_probe_events(&self) -> bool {
        false
    }

    fn no_event_reason(&self) -> Option<String> {
        Some("probe attach is not available on this host".to_string())
    }

    fn attach_probe(
        &mut self,
        _probe: &PlannedProbe,
        _config: &ProbeReaderConfig,
    ) -> ProbeAttachmentStatus {
        ProbeAttachmentStatus::Failed("probe attach is not available on this host".to_string())
    }

    fn poll_events(
        &mut self,
        _max_events: usize,
        _timeout_ms: u64,
    ) -> Result<Vec<ProbeEvent>, SourceError> {
        Ok(Vec::new())
    }

    fn stop(&mut self) -> Result<String, SourceError> {
        Ok("probe driver stopped without attachments".to_string())
    }
}

pub struct MockEventSource {
    name: String,
    events: VecDeque<SourceEvent>,
}

impl MockEventSource {
    pub fn new(name: impl Into<String>, events: Vec<SourceEvent>) -> Self {
        Self {
            name: name.into(),
            events: events.into(),
        }
    }

    pub fn demo_sequence() -> Self {
        Self::new(
            "mock-demo",
            vec![
                SourceEvent::new(1_000, 4_242, SignalKind::RunQueueDelay, 2_500)
                    .with_process_name("ollama")
                    .with_cmdline("ollama serve")
                    .with_cgroup("/aegisai/inference"),
                SourceEvent::new(1_200, 4_242, SignalKind::OffCpuTime, 3_200)
                    .with_process_name("ollama")
                    .with_cmdline("ollama serve")
                    .with_cgroup("/aegisai/inference"),
                SourceEvent::new(2_000, 5_151, SignalKind::QueueWait, 2_700)
                    .with_process_name("python")
                    .with_cmdline("python tool-executor retrieval-worker")
                    .with_parent_pid(4_242)
                    .with_parent_process_name("ollama")
                    .with_parent_cmdline("ollama serve"),
            ],
        )
    }

    pub fn tool_call_lifecycle_sequence() -> Self {
        let lifecycle_id = "tc-001";
        let lifecycle_tag = format!("tool_call_id={lifecycle_id}");
        let executor_cmdline = format!("python tool-executor --tool-call-id {lifecycle_id}");

        Self::new(
            "mock-tool-call-lifecycle",
            vec![
                SourceEvent::new(10_000, 6_100, SignalKind::SubprocessStartDelay, 1_800)
                    .with_process_name("python")
                    .with_cmdline(executor_cmdline.clone())
                    .with_cgroup(format!("/aegisai/tool-call/{lifecycle_id}/executor"))
                    .with_parent_pid(4_242)
                    .with_parent_process_name("ollama")
                    .with_parent_cmdline("ollama serve")
                    .with_tag_marker(lifecycle_tag.clone()),
                SourceEvent::new(10_120, 6_101, SignalKind::QueueWait, 2_600)
                    .with_process_name("python")
                    .with_cmdline(format!(
                        "python tool-executor retrieval-worker --tool-call-id {lifecycle_id}"
                    ))
                    .with_cgroup(format!("/aegisai/tool-call/{lifecycle_id}/retrieval"))
                    .with_parent_pid(6_100)
                    .with_parent_process_name("python")
                    .with_parent_cmdline(executor_cmdline.clone())
                    .with_tag_marker(lifecycle_tag.clone()),
                SourceEvent::new(10_260, 6_101, SignalKind::IoLatency, 4_500)
                    .with_process_name("python")
                    .with_cmdline(format!(
                        "python tool-executor retrieval-worker --tool-call-id {lifecycle_id}"
                    ))
                    .with_cgroup(format!("/aegisai/tool-call/{lifecycle_id}/retrieval"))
                    .with_parent_pid(6_100)
                    .with_parent_process_name("python")
                    .with_parent_cmdline(executor_cmdline.clone())
                    .with_tag_marker(lifecycle_tag.clone()),
                SourceEvent::new(10_360, 6_102, SignalKind::QueueWait, 2_400)
                    .with_process_name("python")
                    .with_cmdline(format!(
                        "python tool-executor rerank-worker --tool-call-id {lifecycle_id}"
                    ))
                    .with_cgroup(format!("/aegisai/tool-call/{lifecycle_id}/rerank"))
                    .with_parent_pid(6_100)
                    .with_parent_process_name("python")
                    .with_parent_cmdline(executor_cmdline.clone())
                    .with_tag_marker(lifecycle_tag.clone()),
                SourceEvent::new(10_420, 6_200, SignalKind::RunQueueDelay, 3_000)
                    .with_process_name("stress-ng")
                    .with_cmdline(format!("stress-ng --cpu 1 --tool-call-id {lifecycle_id}"))
                    .with_cgroup(format!("/aegisai/tool-call/{lifecycle_id}/background"))
                    .with_parent_pid(6_100)
                    .with_parent_process_name("python")
                    .with_parent_cmdline(executor_cmdline)
                    .with_tag_marker(lifecycle_tag),
            ],
        )
    }
}

impl EventSource for MockEventSource {
    fn source_name(&self) -> &str {
        &self.name
    }

    fn next_event(&mut self) -> Result<Option<SourceEvent>, SourceError> {
        Ok(self.events.pop_front())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlannedProbe {
    pub kind: ProbeKind,
    pub descriptor_name: String,
    pub required_signals: BTreeSet<SignalKind>,
    pub attach_points: Vec<AttachPoint>,
    pub config: ProbeConfig,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LinuxProbePlan {
    pub probes: Vec<PlannedProbe>,
    pub runtime_only_signals: BTreeSet<SignalKind>,
}

impl LinuxProbePlan {
    pub fn from_runtime(runtime: &RuntimeConfig) -> Result<Self, SourceError> {
        Self::from_signals(
            runtime.focus_signals.iter().cloned(),
            &ProbeRegistry::with_defaults(),
        )
    }

    pub fn from_signals<I>(signals: I, registry: &ProbeRegistry) -> Result<Self, SourceError>
    where
        I: IntoIterator<Item = SignalKind>,
    {
        let mut probes_by_kind = std::collections::BTreeMap::<ProbeKind, PlannedProbe>::new();
        let mut runtime_only_signals = BTreeSet::new();

        for signal in signals {
            match signal_to_probe_kind(&signal) {
                Some(kind) => {
                    let descriptor = registry.get(kind).ok_or_else(|| {
                        SourceError::InvalidConfig(format!(
                            "probe registry is missing descriptor for {}",
                            kind.as_str()
                        ))
                    })?;

                    let entry = probes_by_kind.entry(kind).or_insert_with(|| PlannedProbe {
                        kind,
                        descriptor_name: descriptor.name.to_string(),
                        required_signals: BTreeSet::new(),
                        attach_points: descriptor.attach_points.clone(),
                        config: descriptor.default_config.clone(),
                    });
                    entry.required_signals.insert(signal);
                }
                None => {
                    runtime_only_signals.insert(signal);
                }
            }
        }

        Ok(Self {
            probes: probes_by_kind.into_values().collect(),
            runtime_only_signals,
        })
    }

    pub fn probe_names(&self) -> Vec<&str> {
        self.probes
            .iter()
            .map(|probe| probe.descriptor_name.as_str())
            .collect()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProbeReaderConfig {
    pub require_all_probes: bool,
    pub max_buffered_events: usize,
    pub poll_timeout_ms: u64,
}

impl Default for ProbeReaderConfig {
    fn default() -> Self {
        Self {
            require_all_probes: true,
            max_buffered_events: 4_096,
            poll_timeout_ms: 100,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProbeAttachmentStatus {
    Attached,
    Failed(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProbeAttachment {
    pub kind: ProbeKind,
    pub descriptor_name: String,
    pub required_signals: BTreeSet<SignalKind>,
    pub status: ProbeAttachmentStatus,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProbeReaderStartup {
    pub reader_name: String,
    pub config: ProbeReaderConfig,
    pub attachments: Vec<ProbeAttachment>,
    pub runtime_only_signals: BTreeSet<SignalKind>,
    pub emits_probe_events: bool,
    pub no_event_reason: Option<String>,
}

impl ProbeReaderStartup {
    fn from_plan<F>(
        reader_name: impl Into<String>,
        plan: &LinuxProbePlan,
        config: &ProbeReaderConfig,
        mut status_for_probe: F,
    ) -> Self
    where
        F: FnMut(&PlannedProbe) -> ProbeAttachmentStatus,
    {
        Self {
            reader_name: reader_name.into(),
            config: config.clone(),
            attachments: plan
                .probes
                .iter()
                .map(|probe| ProbeAttachment {
                    kind: probe.kind,
                    descriptor_name: probe.descriptor_name.clone(),
                    required_signals: probe.required_signals.clone(),
                    status: status_for_probe(probe),
                })
                .collect(),
            runtime_only_signals: plan.runtime_only_signals.clone(),
            emits_probe_events: true,
            no_event_reason: None,
        }
    }

    fn failed_required_probes(&self) -> Vec<String> {
        self.attachments
            .iter()
            .filter_map(|attachment| match &attachment.status {
                ProbeAttachmentStatus::Attached => None,
                ProbeAttachmentStatus::Failed(reason) => Some(format!(
                    "{}({}): {reason}",
                    attachment.descriptor_name,
                    attachment.kind.as_str()
                )),
            })
            .collect()
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProbeReaderShutdown {
    pub reader_name: String,
    pub emitted_events: u64,
    pub stop_reason: String,
}

pub struct LinuxProbeSource {
    plan: LinuxProbePlan,
    reader_config: ProbeReaderConfig,
    reader: Box<dyn ProbeEventReader>,
    startup: Option<ProbeReaderStartup>,
    shutdown: Option<ProbeReaderShutdown>,
}

impl LinuxProbeSource {
    pub fn new(plan: LinuxProbePlan) -> Self {
        Self::with_reader_and_config(
            plan,
            UnsupportedProbeEventReader,
            ProbeReaderConfig::default(),
        )
    }

    pub fn with_reader<R>(plan: LinuxProbePlan, reader: R) -> Self
    where
        R: ProbeEventReader + 'static,
    {
        Self::with_reader_and_config(plan, reader, ProbeReaderConfig::default())
    }

    pub fn with_reader_and_config<R>(
        plan: LinuxProbePlan,
        reader: R,
        reader_config: ProbeReaderConfig,
    ) -> Self
    where
        R: ProbeEventReader + 'static,
    {
        Self {
            plan,
            reader_config,
            reader: Box::new(reader),
            startup: None,
            shutdown: None,
        }
    }

    pub fn from_runtime(runtime: &RuntimeConfig) -> Result<Self, SourceError> {
        Self::from_runtime_with_config(runtime, ProbeReaderConfig::default())
    }

    pub fn from_runtime_with_config(
        runtime: &RuntimeConfig,
        reader_config: ProbeReaderConfig,
    ) -> Result<Self, SourceError> {
        Ok(Self::with_reader_and_config(
            LinuxProbePlan::from_runtime(runtime)?,
            DriverBackedProbeEventReader::new(RealLinuxProbeDriver::from_runtime(runtime)),
            reader_config,
        ))
    }

    pub fn preflight_from_runtime_with_config(
        runtime: &RuntimeConfig,
        reader_config: ProbeReaderConfig,
    ) -> Result<Self, SourceError> {
        Ok(Self::with_reader_and_config(
            LinuxProbePlan::from_runtime(runtime)?,
            DriverBackedProbeEventReader::new(PreflightLinuxProbeDriver::system()),
            reader_config,
        ))
    }

    pub fn plan(&self) -> &LinuxProbePlan {
        &self.plan
    }

    pub fn reader_config(&self) -> &ProbeReaderConfig {
        &self.reader_config
    }

    pub fn startup(&self) -> Option<&ProbeReaderStartup> {
        self.startup.as_ref()
    }

    pub fn shutdown(&self) -> Option<&ProbeReaderShutdown> {
        self.shutdown.as_ref()
    }

    pub fn stop(&mut self) -> Result<&ProbeReaderShutdown, SourceError> {
        if self.shutdown.is_none() {
            let shutdown = self.reader.stop()?;
            self.shutdown = Some(shutdown);
        }

        Ok(self.shutdown.as_ref().expect("shutdown was just recorded"))
    }

    fn ensure_started(&mut self) -> Result<(), SourceError> {
        if self.startup.is_some() {
            return Ok(());
        }

        if self.reader_config.max_buffered_events == 0 {
            return Err(SourceError::InvalidConfig(
                "probe reader max_buffered_events must be greater than 0".to_string(),
            ));
        }

        let startup = self.reader.start(&self.plan, &self.reader_config)?;
        let failed_required = startup.failed_required_probes();
        if self.reader_config.require_all_probes && !failed_required.is_empty() {
            return Err(SourceError::Unsupported(format!(
                "linux probe reader `{}` could not attach required probes: {}; planned probes: [{}]; runtime-only signals: [{}]",
                startup.reader_name,
                failed_required.join(", "),
                self.plan.probe_names().join(", "),
                self.plan
                    .runtime_only_signals
                    .iter()
                    .map(SignalKind::as_str)
                    .collect::<Vec<_>>()
                    .join(", ")
            )));
        }

        self.startup = Some(startup);
        Ok(())
    }
}

impl EventSource for LinuxProbeSource {
    fn source_name(&self) -> &str {
        "linux-probe"
    }

    fn poll_batch(&mut self, max_batch: usize) -> Result<Vec<SourceEvent>, SourceError> {
        if max_batch == 0 {
            return Err(SourceError::InvalidConfig(
                "event source batch size must be greater than 0".to_string(),
            ));
        }

        self.ensure_started()?;
        let probe_events = self.reader.poll_probe_events(max_batch)?;
        let mut events = Vec::with_capacity(probe_events.len());
        for probe_event in probe_events {
            if let Some(source_event) = adapt_probe_event(probe_event)? {
                events.push(source_event);
            }
        }

        Ok(events)
    }

    fn next_event(&mut self) -> Result<Option<SourceEvent>, SourceError> {
        self.ensure_started()?;

        loop {
            match self.reader.next_probe_event() {
                Ok(Some(event)) => {
                    if let Some(source_event) = adapt_probe_event(event)? {
                        return Ok(Some(source_event));
                    }
                }
                Ok(None) => return Ok(None),
                Err(SourceError::Unsupported(message)) => {
                    return Err(SourceError::Unsupported(message))
                }
                Err(error) => return Err(error),
            }
        }
    }
}

fn signal_to_probe_kind(signal: &SignalKind) -> Option<ProbeKind> {
    match signal {
        SignalKind::RunQueueDelay | SignalKind::CpuMigration => Some(ProbeKind::Sched),
        SignalKind::OffCpuTime => Some(ProbeKind::OffCpu),
        SignalKind::MajorPageFault => Some(ProbeKind::Fault),
        SignalKind::IoLatency => Some(ProbeKind::Io),
        SignalKind::SubprocessStartDelay | SignalKind::QueueWait | SignalKind::Unknown(_) => None,
    }
}

#[derive(Default)]
pub struct UnsupportedProbeEventReader;

impl ProbeEventReader for UnsupportedProbeEventReader {
    fn reader_name(&self) -> &str {
        "unsupported"
    }

    fn start(
        &mut self,
        plan: &LinuxProbePlan,
        config: &ProbeReaderConfig,
    ) -> Result<ProbeReaderStartup, SourceError> {
        let mut startup = ProbeReaderStartup::from_plan(self.reader_name(), plan, config, |_| {
            ProbeAttachmentStatus::Failed("reader is not wired yet on this host".to_string())
        });
        startup.emits_probe_events = false;
        startup.no_event_reason = Some(
            "unsupported reader is a planning placeholder and emits no probe events".to_string(),
        );
        Ok(startup)
    }

    fn next_probe_event(&mut self) -> Result<Option<ProbeEvent>, SourceError> {
        Ok(None)
    }

    fn stop(&mut self) -> Result<ProbeReaderShutdown, SourceError> {
        Ok(ProbeReaderShutdown {
            reader_name: self.reader_name().to_string(),
            emitted_events: 0,
            stop_reason: "unsupported reader never started".to_string(),
        })
    }
}

pub struct StaticProbeEventReader {
    events: VecDeque<ProbeEvent>,
    started: bool,
    emitted_events: u64,
}

impl StaticProbeEventReader {
    pub fn new(events: Vec<ProbeEvent>) -> Self {
        Self {
            events: events.into(),
            started: false,
            emitted_events: 0,
        }
    }
}

impl ProbeEventReader for StaticProbeEventReader {
    fn reader_name(&self) -> &str {
        "static"
    }

    fn start(
        &mut self,
        plan: &LinuxProbePlan,
        config: &ProbeReaderConfig,
    ) -> Result<ProbeReaderStartup, SourceError> {
        self.started = true;
        Ok(ProbeReaderStartup::from_plan(
            self.reader_name(),
            plan,
            config,
            |_| ProbeAttachmentStatus::Attached,
        ))
    }

    fn next_probe_event(&mut self) -> Result<Option<ProbeEvent>, SourceError> {
        if !self.started {
            return Err(SourceError::InvalidConfig(
                "probe reader must be started before polling events".to_string(),
            ));
        }

        let event = self.events.pop_front();
        if event.is_some() {
            self.emitted_events = self.emitted_events.saturating_add(1);
        }
        Ok(event)
    }

    fn stop(&mut self) -> Result<ProbeReaderShutdown, SourceError> {
        self.started = false;
        Ok(ProbeReaderShutdown {
            reader_name: self.reader_name().to_string(),
            emitted_events: self.emitted_events,
            stop_reason: "reader drained".to_string(),
        })
    }
}

fn adapt_probe_event(event: ProbeEvent) -> Result<Option<SourceEvent>, SourceError> {
    let signal = match event.kind {
        ProbeEventKind::RunQueueDelay => SignalKind::RunQueueDelay,
        ProbeEventKind::CpuMigration => SignalKind::CpuMigration,
        ProbeEventKind::OffCpuDuration => SignalKind::OffCpuTime,
        ProbeEventKind::MajorPageFault => SignalKind::MajorPageFault,
        ProbeEventKind::BlockIoLatency => SignalKind::IoLatency,
        ProbeEventKind::ContextSwitch
        | ProbeEventKind::MinorPageFault
        | ProbeEventKind::IoBytes => return Ok(None),
    };

    let value = normalize_probe_metric(signal.as_str(), event.metric.unit, event.metric.value)?;
    let mut source_event = SourceEvent::new(
        ns_to_ms_ceil(event.timestamp_ns),
        event.target.pid,
        signal,
        value,
    )
    .with_tid(event.target.tid)
    .with_process_name(event.target.comm);

    if let Some(cgroup_id) = event.target.cgroup_id {
        source_event = source_event.with_cgroup(format!("cgroup:{cgroup_id}"));
    }

    Ok(Some(source_event))
}

fn normalize_probe_metric(
    signal_name: &str,
    unit: MetricUnit,
    value: u64,
) -> Result<u64, SourceError> {
    let normalized = match unit {
        MetricUnit::DurationNs => ns_to_us_ceil(value),
        MetricUnit::Count | MetricUnit::Pages | MetricUnit::Bytes => value,
    };

    if matches!(unit, MetricUnit::Bytes) && signal_name != SignalKind::IoLatency.as_str() {
        return Err(SourceError::InvalidConfig(format!(
            "unexpected byte metric for signal `{signal_name}`"
        )));
    }

    Ok(normalized)
}

fn ns_to_ms_ceil(value: u64) -> u64 {
    value.saturating_add(999_999) / 1_000_000
}

fn ns_to_us_ceil(value: u64) -> u64 {
    value.saturating_add(999) / 1_000
}

fn parse_schedstat_run_delay_ns(raw: &str) -> Option<u64> {
    raw.split_whitespace().nth(1)?.parse::<u64>().ok()
}

fn parse_sched_value(raw: &str, key: &str) -> Option<u64> {
    raw.lines().find_map(|line| {
        let (name, value) = line.split_once(':')?;
        if name.trim() == key {
            value.split_whitespace().next()?.parse::<u64>().ok()
        } else {
            None
        }
    })
}

fn parse_stat_major_page_faults(raw: &str) -> Option<u64> {
    let mut fields_after_comm = raw.get(raw.rfind(')')? + 1..)?.split_whitespace();
    fields_after_comm.nth(9)?.parse::<u64>().ok()
}

fn format_cmdline(bytes: Vec<u8>) -> String {
    bytes
        .split(|byte| *byte == 0)
        .filter(|item| !item.is_empty())
        .map(|item| String::from_utf8_lossy(item).to_string())
        .collect::<Vec<_>>()
        .join(" ")
}

fn now_ns() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos().min(u128::from(u64::MAX)) as u64)
        .unwrap_or(1)
        .max(1)
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::collections::BTreeSet;
    use std::collections::VecDeque;

    use ebpf_probe::{AttachPoint, EventMetric, EventTarget, ProbeKind};

    use super::{
        BpfTraceProbeDriver, DriverBackedProbeEventReader, EventSource, FakeBpfTracePipe,
        LinuxProbeDriver, LinuxProbeHost, LinuxProbePlan, LinuxProbeSource, MockEventSource,
        PreflightLinuxProbeDriver, ProbeAttachmentStatus, ProbeReaderConfig,
        ProcfsSchedstatProbeDriver, ProcfsSchedstatSampler, ProcfsSchedstatSnapshot,
        ProcfsTargetSelectors, RealLinuxProbeDriver, SignalKind, SourceError, SourceEvent,
        StaticProbeEventReader,
    };

    struct FakeLinuxProbeDriver {
        events: VecDeque<ebpf_probe::Event>,
        attached: Vec<String>,
        stopped: bool,
    }

    impl FakeLinuxProbeDriver {
        fn new(events: Vec<ebpf_probe::Event>) -> Self {
            Self {
                events: events.into(),
                attached: Vec::new(),
                stopped: false,
            }
        }
    }

    impl LinuxProbeDriver for FakeLinuxProbeDriver {
        fn driver_name(&self) -> &str {
            "fake-probe-driver"
        }

        fn attach_probe(
            &mut self,
            probe: &super::PlannedProbe,
            _config: &ProbeReaderConfig,
        ) -> ProbeAttachmentStatus {
            self.attached.push(probe.descriptor_name.clone());
            ProbeAttachmentStatus::Attached
        }

        fn poll_events(
            &mut self,
            max_events: usize,
            _timeout_ms: u64,
        ) -> Result<Vec<ebpf_probe::Event>, SourceError> {
            let mut batch = Vec::new();
            while batch.len() < max_events {
                let Some(event) = self.events.pop_front() else {
                    break;
                };
                batch.push(event);
            }

            Ok(batch)
        }

        fn stop(&mut self) -> Result<String, SourceError> {
            self.stopped = true;
            Ok("driver stopped".to_string())
        }
    }

    struct ChunkedLinuxProbeDriver {
        chunks: VecDeque<Vec<ebpf_probe::Event>>,
    }

    impl ChunkedLinuxProbeDriver {
        fn new(chunks: Vec<Vec<ebpf_probe::Event>>) -> Self {
            Self {
                chunks: chunks.into(),
            }
        }
    }

    impl LinuxProbeDriver for ChunkedLinuxProbeDriver {
        fn driver_name(&self) -> &str {
            "chunked-probe-driver"
        }

        fn attach_probe(
            &mut self,
            _probe: &super::PlannedProbe,
            _config: &ProbeReaderConfig,
        ) -> ProbeAttachmentStatus {
            ProbeAttachmentStatus::Attached
        }

        fn poll_events(
            &mut self,
            _max_events: usize,
            _timeout_ms: u64,
        ) -> Result<Vec<ebpf_probe::Event>, SourceError> {
            Ok(self.chunks.pop_front().unwrap_or_default())
        }

        fn stop(&mut self) -> Result<String, SourceError> {
            Ok("chunked driver stopped".to_string())
        }
    }

    struct FakeProcfsSignalSampler {
        samples: RefCell<VecDeque<Vec<ProcfsSchedstatSnapshot>>>,
    }

    impl FakeProcfsSignalSampler {
        fn new(samples: Vec<Vec<ProcfsSchedstatSnapshot>>) -> Self {
            Self {
                samples: RefCell::new(samples.into()),
            }
        }
    }

    impl ProcfsSchedstatSampler for FakeProcfsSignalSampler {
        fn sampler_name(&self) -> &str {
            "fake-procfs-signals"
        }

        fn sample(
            &self,
            _selectors: &ProcfsTargetSelectors,
        ) -> Result<Vec<ProcfsSchedstatSnapshot>, SourceError> {
            Ok(self.samples.borrow_mut().pop_front().unwrap_or_default())
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn system_procfs_sampler_reads_migration_and_fault_counters() {
        let sampler = super::SystemProcfsSchedstatSampler;
        let selectors = ProcfsTargetSelectors::new(
            Vec::<String>::new(),
            [std::process::id()].into_iter().collect(),
        );

        let snapshots = sampler
            .sample(&selectors)
            .expect("system procfs sampler should read current process");

        assert!(snapshots.iter().any(|snapshot| {
            snapshot.pid == std::process::id()
                && snapshot.cpu_migrations.is_some()
                && snapshot.major_page_faults.is_some()
        }));
    }

    struct FakeLinuxProbeHost {
        supported: BTreeSet<String>,
    }

    impl FakeLinuxProbeHost {
        fn new<I, S>(supported: I) -> Self
        where
            I: IntoIterator<Item = S>,
            S: Into<String>,
        {
            Self {
                supported: supported.into_iter().map(Into::into).collect(),
            }
        }
    }

    impl LinuxProbeHost for FakeLinuxProbeHost {
        fn host_name(&self) -> &str {
            "fake-linux-host"
        }

        fn supports_attach_point(&self, attach_point: &AttachPoint) -> Result<(), String> {
            let key = match attach_point {
                AttachPoint::TracePoint { category, name } => {
                    format!("tracepoint:{category}/{name}")
                }
                AttachPoint::KProbe { function } => format!("kprobe:{function}"),
                AttachPoint::KRetProbe { function } => format!("kretprobe:{function}"),
                AttachPoint::RawTracePoint { name } => format!("raw_tracepoint:{name}"),
            };

            if self.supported.contains(&key) {
                Ok(())
            } else {
                Err(format!("missing {key}"))
            }
        }
    }

    #[test]
    fn poll_batch_collects_up_to_requested_events() {
        let mut source = MockEventSource::new(
            "batch-test",
            vec![
                SourceEvent::new(1, 1, SignalKind::RunQueueDelay, 10),
                SourceEvent::new(2, 1, SignalKind::OffCpuTime, 20),
                SourceEvent::new(3, 1, SignalKind::MajorPageFault, 1),
            ],
        );

        let batch = source.poll_batch(2).expect("batch should succeed");
        assert_eq!(batch.len(), 2);

        let remaining = source.poll_batch(2).expect("batch should succeed");
        assert_eq!(remaining.len(), 1);
    }

    #[test]
    fn zero_batch_size_is_rejected() {
        let mut source = MockEventSource::new("batch-test", Vec::new());
        assert!(matches!(
            source.poll_batch(0),
            Err(SourceError::InvalidConfig(_))
        ));
    }

    #[test]
    fn linux_probe_plan_maps_focus_signals_to_required_probe_set() {
        let plan = LinuxProbePlan::from_signals(
            [
                SignalKind::RunQueueDelay,
                SignalKind::OffCpuTime,
                SignalKind::CpuMigration,
                SignalKind::MajorPageFault,
                SignalKind::QueueWait,
            ],
            &ebpf_probe::ProbeRegistry::with_defaults(),
        )
        .expect("plan should build");

        assert_eq!(plan.probes.len(), 3);
        assert!(plan
            .probes
            .iter()
            .any(|probe| probe.kind == ProbeKind::Sched));
        assert!(plan
            .probes
            .iter()
            .any(|probe| probe.kind == ProbeKind::OffCpu));
        assert!(plan
            .probes
            .iter()
            .any(|probe| probe.kind == ProbeKind::Fault));
        assert!(plan.runtime_only_signals.contains(&SignalKind::QueueWait));
    }

    #[test]
    fn probe_event_adapter_maps_sched_delay_to_source_event() {
        let plan = LinuxProbePlan::from_signals(
            [SignalKind::RunQueueDelay],
            &ebpf_probe::ProbeRegistry::with_defaults(),
        )
        .expect("plan should build");
        let event = ebpf_probe::Event::new(
            2_400_000,
            ProbeKind::Sched,
            ebpf_probe::EventKind::RunQueueDelay,
            EventTarget::new(77, 78, "ollama"),
            EventMetric::duration_ns(2_500_000),
        );
        let mut source =
            LinuxProbeSource::with_reader(plan, StaticProbeEventReader::new(vec![event]));

        let adapted = source
            .next_event()
            .expect("adapter should succeed")
            .expect("one event should be produced");

        assert_eq!(adapted.timestamp_ms, 3);
        assert_eq!(adapted.value, 2_500);
        assert_eq!(adapted.process_name.as_deref(), Some("ollama"));
        assert_eq!(adapted.tid, Some(78));
    }

    #[test]
    fn linux_probe_source_starts_reader_and_records_startup_state() {
        let plan = LinuxProbePlan::from_signals(
            [SignalKind::RunQueueDelay, SignalKind::OffCpuTime],
            &ebpf_probe::ProbeRegistry::with_defaults(),
        )
        .expect("plan should build");
        let event = ebpf_probe::Event::new(
            1_000_000,
            ProbeKind::Sched,
            ebpf_probe::EventKind::RunQueueDelay,
            EventTarget::new(77, 78, "ollama"),
            EventMetric::duration_ns(2_000_000),
        );
        let mut source =
            LinuxProbeSource::with_reader(plan, StaticProbeEventReader::new(vec![event]));

        let _ = source
            .next_event()
            .expect("reader should start and adapt the first event");

        let startup = source.startup().expect("startup state should be recorded");
        assert_eq!(startup.reader_name, "static");
        assert_eq!(startup.attachments.len(), 2);
        assert!(startup
            .attachments
            .iter()
            .all(|attachment| attachment.status == ProbeAttachmentStatus::Attached));

        let shutdown = source.stop().expect("reader shutdown should succeed");
        assert_eq!(shutdown.reader_name, "static");
        assert_eq!(shutdown.emitted_events, 1);
    }

    #[test]
    fn unsupported_probe_reader_reports_failed_required_probes() {
        let plan = LinuxProbePlan::from_signals(
            [SignalKind::RunQueueDelay],
            &ebpf_probe::ProbeRegistry::with_defaults(),
        )
        .expect("plan should build");
        let mut source = LinuxProbeSource::new(plan);

        let error = source
            .next_event()
            .expect_err("unsupported reader should fail during startup");

        assert!(matches!(error, SourceError::Unsupported(_)));
        let message = error.to_string();
        assert!(message.contains("could not attach required probes"));
        assert!(message.contains("sched_probe"));
    }

    #[test]
    fn zero_buffered_probe_config_is_rejected_before_reader_start() {
        let plan = LinuxProbePlan::from_signals(
            [SignalKind::RunQueueDelay],
            &ebpf_probe::ProbeRegistry::with_defaults(),
        )
        .expect("plan should build");
        let mut source = LinuxProbeSource::with_reader_and_config(
            plan,
            StaticProbeEventReader::new(Vec::new()),
            ProbeReaderConfig {
                max_buffered_events: 0,
                ..ProbeReaderConfig::default()
            },
        );

        let error = source
            .next_event()
            .expect_err("zero buffered events should be rejected");

        assert!(matches!(error, SourceError::InvalidConfig(_)));
    }

    #[test]
    fn driver_backed_reader_attaches_polls_and_stops() {
        let plan = LinuxProbePlan::from_signals(
            [SignalKind::RunQueueDelay, SignalKind::OffCpuTime],
            &ebpf_probe::ProbeRegistry::with_defaults(),
        )
        .expect("plan should build");
        let event = ebpf_probe::Event::new(
            2_000_000,
            ProbeKind::Sched,
            ebpf_probe::EventKind::RunQueueDelay,
            EventTarget::new(700, 701, "ollama"),
            EventMetric::duration_ns(1_800_000),
        );
        let reader = DriverBackedProbeEventReader::new(FakeLinuxProbeDriver::new(vec![event]));
        let mut source = LinuxProbeSource::with_reader(plan, reader);

        let adapted = source
            .next_event()
            .expect("driver-backed reader should start and poll")
            .expect("event should be produced");
        assert_eq!(adapted.pid, 700);
        assert_eq!(adapted.value, 1_800);

        let startup = source.startup().expect("startup should exist");
        assert_eq!(startup.reader_name, "fake-probe-driver");
        assert_eq!(startup.attachments.len(), 2);
        assert!(startup.emits_probe_events);
        assert_eq!(startup.no_event_reason, None);

        let shutdown = source.stop().expect("shutdown should succeed");
        assert_eq!(shutdown.stop_reason, "driver stopped");
        assert_eq!(shutdown.emitted_events, 1);
    }

    #[test]
    fn linux_probe_source_batch_uses_one_driver_poll_at_a_time() {
        let plan = LinuxProbePlan::from_signals(
            [SignalKind::RunQueueDelay],
            &ebpf_probe::ProbeRegistry::with_defaults(),
        )
        .expect("plan should build");
        let first = ebpf_probe::Event::new(
            2_000_000,
            ProbeKind::Sched,
            ebpf_probe::EventKind::RunQueueDelay,
            EventTarget::new(700, 701, "ollama"),
            EventMetric::duration_ns(1_800_000),
        );
        let second = ebpf_probe::Event::new(
            2_100_000,
            ProbeKind::Sched,
            ebpf_probe::EventKind::RunQueueDelay,
            EventTarget::new(700, 701, "ollama"),
            EventMetric::duration_ns(1_900_000),
        );
        let reader = DriverBackedProbeEventReader::new(ChunkedLinuxProbeDriver::new(vec![
            vec![first],
            vec![second],
        ]));
        let mut source = LinuxProbeSource::with_reader(plan, reader);

        let first_batch = source.poll_batch(2).expect("batch should poll once");
        assert_eq!(first_batch.len(), 1);
        assert_eq!(first_batch[0].value, 1_800);

        let second_batch = source.poll_batch(2).expect("second batch should poll once");
        assert_eq!(second_batch.len(), 1);
        assert_eq!(second_batch[0].value, 1_900);
    }

    #[test]
    fn procfs_schedstat_driver_emits_run_queue_delay_events() {
        let plan = LinuxProbePlan::from_signals(
            [SignalKind::RunQueueDelay, SignalKind::OffCpuTime],
            &ebpf_probe::ProbeRegistry::with_defaults(),
        )
        .expect("plan should build");
        let sampler = FakeProcfsSignalSampler::new(vec![
            vec![ProcfsSchedstatSnapshot {
                timestamp_ns: 1_000_000,
                pid: 700,
                tid: 700,
                comm: "ollama".to_string(),
                run_queue_delay_ns: Some(2_000_000),
                cpu_migrations: None,
                major_page_faults: None,
            }],
            vec![ProcfsSchedstatSnapshot {
                timestamp_ns: 2_000_000,
                pid: 700,
                tid: 700,
                comm: "ollama".to_string(),
                run_queue_delay_ns: Some(2_500_000),
                cpu_migrations: None,
                major_page_faults: None,
            }],
        ]);
        let driver = ProcfsSchedstatProbeDriver::new(
            ProcfsTargetSelectors::new(["ollama"], BTreeSet::new()),
            sampler,
        );
        let reader = DriverBackedProbeEventReader::new(driver);
        let mut source = LinuxProbeSource::with_reader_and_config(
            plan,
            reader,
            ProbeReaderConfig {
                require_all_probes: false,
                poll_timeout_ms: 1,
                ..ProbeReaderConfig::default()
            },
        );

        let first = source
            .next_event()
            .expect("procfs driver should poll")
            .expect("second schedstat sample should produce a delta event");
        assert_eq!(first.pid, 700);
        assert_eq!(first.signal, SignalKind::RunQueueDelay);
        assert_eq!(first.value, 500);
        assert_eq!(first.process_name.as_deref(), Some("ollama"));

        let startup = source.startup().expect("startup should exist");
        assert_eq!(startup.reader_name, "procfs-schedstat-driver");
        assert!(startup
            .attachments
            .iter()
            .any(|attachment| attachment.kind == ProbeKind::Sched
                && attachment.status == ProbeAttachmentStatus::Attached));
        assert!(startup
            .attachments
            .iter()
            .any(|attachment| attachment.kind == ProbeKind::OffCpu
                && matches!(attachment.status, ProbeAttachmentStatus::Failed(_))));
        assert!(startup.emits_probe_events);

        assert!(source
            .next_event()
            .expect("procfs driver should poll again")
            .is_none());
    }

    #[test]
    fn procfs_driver_emits_migration_and_major_fault_events() {
        let plan = LinuxProbePlan::from_signals(
            [
                SignalKind::CpuMigration,
                SignalKind::MajorPageFault,
                SignalKind::OffCpuTime,
            ],
            &ebpf_probe::ProbeRegistry::with_defaults(),
        )
        .expect("plan should build");
        let sampler = FakeProcfsSignalSampler::new(vec![
            vec![ProcfsSchedstatSnapshot {
                timestamp_ns: 1_000_000,
                pid: 700,
                tid: 700,
                comm: "ollama".to_string(),
                run_queue_delay_ns: None,
                cpu_migrations: Some(4),
                major_page_faults: Some(2),
            }],
            vec![ProcfsSchedstatSnapshot {
                timestamp_ns: 2_000_000,
                pid: 700,
                tid: 700,
                comm: "ollama".to_string(),
                run_queue_delay_ns: None,
                cpu_migrations: Some(7),
                major_page_faults: Some(4),
            }],
        ]);
        let driver = ProcfsSchedstatProbeDriver::new(
            ProcfsTargetSelectors::new(["ollama"], BTreeSet::new()),
            sampler,
        );
        let reader = DriverBackedProbeEventReader::new(driver);
        let mut source = LinuxProbeSource::with_reader_and_config(
            plan,
            reader,
            ProbeReaderConfig {
                require_all_probes: false,
                poll_timeout_ms: 1,
                ..ProbeReaderConfig::default()
            },
        );

        let first = source
            .next_event()
            .expect("procfs driver should poll")
            .expect("second procfs sample should produce a migration event");
        let second = source
            .next_event()
            .expect("procfs driver should keep buffered events")
            .expect("second procfs sample should produce a fault event");

        assert_eq!(first.signal, SignalKind::CpuMigration);
        assert_eq!(first.value, 3);
        assert_eq!(second.signal, SignalKind::MajorPageFault);
        assert_eq!(second.value, 2);

        let startup = source.startup().expect("startup should exist");
        assert!(startup
            .attachments
            .iter()
            .any(|attachment| attachment.kind == ProbeKind::Sched
                && attachment
                    .required_signals
                    .contains(&SignalKind::CpuMigration)
                && attachment.status == ProbeAttachmentStatus::Attached));
        assert!(startup
            .attachments
            .iter()
            .any(|attachment| attachment.kind == ProbeKind::Fault
                && attachment
                    .required_signals
                    .contains(&SignalKind::MajorPageFault)
                && attachment.status == ProbeAttachmentStatus::Attached));
        assert!(startup
            .attachments
            .iter()
            .any(|attachment| attachment.kind == ProbeKind::OffCpu
                && matches!(attachment.status, ProbeAttachmentStatus::Failed(_))));
    }

    #[test]
    fn bpftrace_driver_emits_offcpu_and_io_latency_events() {
        let plan = LinuxProbePlan::from_signals(
            [SignalKind::OffCpuTime, SignalKind::IoLatency],
            &ebpf_probe::ProbeRegistry::with_defaults(),
        )
        .expect("plan should build");
        let pipe = FakeBpfTracePipe::new(vec![vec![
            "noise from tool startup",
            "aegisai_probe signal=offcpu_time ts_ns=2000000 pid=700 tid=701 comm=ollama value_ns=4500000",
            "aegisai_probe signal=io_latency ts_ns=3000000 pid=700 tid=701 comm=ollama value_ns=900000",
        ]]);
        let driver = BpfTraceProbeDriver::new(
            ProcfsTargetSelectors::new(["ollama"], BTreeSet::new()),
            pipe,
        );
        let reader = DriverBackedProbeEventReader::new(driver);
        let mut source = LinuxProbeSource::with_reader_and_config(
            plan,
            reader,
            ProbeReaderConfig {
                poll_timeout_ms: 1,
                ..ProbeReaderConfig::default()
            },
        );

        let offcpu = source
            .next_event()
            .expect("bpftrace driver should poll")
            .expect("offcpu event should be produced");
        let io = source
            .next_event()
            .expect("bpftrace driver should keep buffered events")
            .expect("io event should be produced");

        assert_eq!(offcpu.signal, SignalKind::OffCpuTime);
        assert_eq!(offcpu.value, 4_500);
        assert_eq!(offcpu.timestamp_ms, 2);
        assert_eq!(offcpu.process_name.as_deref(), Some("ollama"));
        assert_eq!(offcpu.tid, Some(701));
        assert_eq!(io.signal, SignalKind::IoLatency);
        assert_eq!(io.value, 900);

        let startup = source.startup().expect("startup should exist");
        assert_eq!(startup.reader_name, "fake-bpftrace");
        assert!(startup.attachments.iter().all(|attachment| {
            matches!(attachment.kind, ProbeKind::OffCpu | ProbeKind::Io)
                && attachment.status == ProbeAttachmentStatus::Attached
        }));
    }

    #[test]
    fn bpftrace_driver_reports_unavailable_attach_reason() {
        let plan = LinuxProbePlan::from_signals(
            [SignalKind::OffCpuTime],
            &ebpf_probe::ProbeRegistry::with_defaults(),
        )
        .expect("plan should build");
        let driver = BpfTraceProbeDriver::new(
            ProcfsTargetSelectors::new(["ollama"], BTreeSet::new()),
            FakeBpfTracePipe::unavailable("bpftrace requires root"),
        );
        let reader = DriverBackedProbeEventReader::new(driver);
        let mut source = LinuxProbeSource::with_reader(plan, reader);

        let error = source
            .next_event()
            .expect_err("unavailable bpftrace should fail startup");

        assert!(matches!(error, SourceError::Unsupported(_)));
        assert!(error.to_string().contains("bpftrace requires root"));
    }

    #[test]
    fn real_linux_probe_driver_combines_procfs_and_bpftrace_signals() {
        let plan = LinuxProbePlan::from_signals(
            [
                SignalKind::RunQueueDelay,
                SignalKind::MajorPageFault,
                SignalKind::OffCpuTime,
                SignalKind::IoLatency,
            ],
            &ebpf_probe::ProbeRegistry::with_defaults(),
        )
        .expect("plan should build");
        let sampler = FakeProcfsSignalSampler::new(vec![
            vec![ProcfsSchedstatSnapshot {
                timestamp_ns: 1_000_000,
                pid: 700,
                tid: 700,
                comm: "ollama".to_string(),
                run_queue_delay_ns: Some(2_000_000),
                cpu_migrations: None,
                major_page_faults: Some(4),
            }],
            vec![ProcfsSchedstatSnapshot {
                timestamp_ns: 2_000_000,
                pid: 700,
                tid: 700,
                comm: "ollama".to_string(),
                run_queue_delay_ns: Some(2_500_000),
                cpu_migrations: None,
                major_page_faults: Some(7),
            }],
        ]);
        let procfs = ProcfsSchedstatProbeDriver::new(
            ProcfsTargetSelectors::new(["ollama"], BTreeSet::new()),
            sampler,
        );
        let bpftrace = BpfTraceProbeDriver::new(
            ProcfsTargetSelectors::new(["ollama"], BTreeSet::new()),
            FakeBpfTracePipe::new(vec![vec![
                "aegisai_probe signal=offcpu_time ts_ns=2000000 pid=700 tid=700 comm=ollama value_ns=3000000",
                "aegisai_probe signal=io_latency ts_ns=2100000 pid=700 tid=700 comm=ollama value_ns=1200000",
            ]]),
        );
        let driver = RealLinuxProbeDriver::new(procfs, bpftrace);
        let reader = DriverBackedProbeEventReader::new(driver);
        let mut source = LinuxProbeSource::with_reader_and_config(
            plan,
            reader,
            ProbeReaderConfig {
                poll_timeout_ms: 1,
                ..ProbeReaderConfig::default()
            },
        );

        let batch = source
            .poll_batch(8)
            .expect("real linux driver should poll bpftrace events and prime procfs");
        let second_batch = source
            .poll_batch(8)
            .expect("second poll should emit procfs deltas");
        let signals = batch
            .iter()
            .chain(second_batch.iter())
            .map(|event| event.signal.clone())
            .collect::<BTreeSet<_>>();

        assert!(signals.contains(&SignalKind::OffCpuTime));
        assert!(signals.contains(&SignalKind::IoLatency));
        assert!(signals.contains(&SignalKind::RunQueueDelay));
        assert!(signals.contains(&SignalKind::MajorPageFault));

        let startup = source.startup().expect("startup should exist");
        assert_eq!(startup.reader_name, "real-linux-probe-driver");
        assert_eq!(startup.attachments.len(), 4);
        assert!(startup
            .attachments
            .iter()
            .all(|attachment| attachment.status == ProbeAttachmentStatus::Attached));
    }

    #[test]
    fn bpftrace_program_scopes_to_configured_targets() {
        let selectors = ProcfsTargetSelectors::new(["ollama"], [42].into_iter().collect());
        let program = super::bpftrace_program(&selectors, true, true);

        assert!(program.contains("tracepoint:sched:sched_switch"));
        assert!(program.contains("tracepoint:block:block_rq_issue"));
        assert!(program.contains("tracepoint:block:block_rq_complete"));
        assert!(program.contains("aegisai_probe signal=offcpu_time"));
        assert!(program.contains("aegisai_probe signal=io_latency"));
        assert!(program.contains("pid == 42"));
        assert!(program.contains("args->prev_pid == 42"));
        assert!(program.contains("comm == \"ollama\""));
    }

    #[test]
    fn procfs_target_selectors_match_process_names_and_pid_allowlist() {
        let selectors = ProcfsTargetSelectors::new(["ollama"], [42].into_iter().collect());

        assert!(selectors.matches(7, "ollama", ""));
        assert!(selectors.matches(8, "python", "python launch_ollama_worker.py"));
        assert!(selectors.matches(42, "python", "python unrelated.py"));
        assert!(!selectors.matches(9, "python", "python unrelated.py"));
    }

    #[test]
    fn procfs_target_selectors_with_only_pid_allowlist_do_not_match_everything() {
        let selectors =
            ProcfsTargetSelectors::new(Vec::<String>::new(), [42].into_iter().collect());

        assert!(selectors.matches(42, "python", "python unrelated.py"));
        assert!(!selectors.matches(7, "ollama", "ollama serve"));
    }

    #[test]
    fn schedstat_and_cmdline_parsers_handle_procfs_shapes() {
        assert_eq!(
            super::parse_schedstat_run_delay_ns("100 2500 3\n"),
            Some(2500)
        );
        assert_eq!(
            super::parse_sched_value(
                "se.nr_migrations                             :                    7\n",
                "se.nr_migrations"
            ),
            Some(7)
        );
        assert_eq!(
            super::parse_stat_major_page_faults("123 (ollama worker) S 1 2 3 4 5 6 10 11 12 13 14"),
            Some(12)
        );
        assert_eq!(
            super::format_cmdline(b"ollama\0serve\0".to_vec()),
            "ollama serve"
        );
    }

    #[test]
    fn preflight_driver_marks_probe_attached_when_host_supports_all_attach_points() {
        let plan = LinuxProbePlan::from_signals(
            [SignalKind::RunQueueDelay, SignalKind::OffCpuTime],
            &ebpf_probe::ProbeRegistry::with_defaults(),
        )
        .expect("plan should build");
        let host = FakeLinuxProbeHost::new([
            "tracepoint:sched/sched_wakeup",
            "tracepoint:sched/sched_switch",
            "tracepoint:sched/sched_migrate_task",
        ]);
        let reader = DriverBackedProbeEventReader::new(PreflightLinuxProbeDriver::new(host));
        let mut source = LinuxProbeSource::with_reader(plan, reader);

        let event = source
            .next_event()
            .expect("preflight reader should start")
            .is_none();
        assert!(event);

        let startup = source.startup().expect("startup should exist");
        assert!(startup
            .attachments
            .iter()
            .all(|attachment| attachment.status == ProbeAttachmentStatus::Attached));
        assert!(!startup.emits_probe_events);
        assert!(startup
            .no_event_reason
            .as_deref()
            .expect("preflight should explain no-event behavior")
            .contains("does not load eBPF programs or read ring buffers"));

        let shutdown = source.stop().expect("shutdown should succeed");
        assert!(shutdown
            .stop_reason
            .contains("attaching 2 probe(s) and rejecting 0 probe(s)"));
    }

    #[test]
    fn preflight_driver_rejects_missing_kprobe_symbol() {
        let plan = LinuxProbePlan::from_signals(
            [SignalKind::MajorPageFault],
            &ebpf_probe::ProbeRegistry::with_defaults(),
        )
        .expect("plan should build");
        let host = FakeLinuxProbeHost::new(Vec::<String>::new());
        let reader = DriverBackedProbeEventReader::new(PreflightLinuxProbeDriver::new(host));
        let mut source = LinuxProbeSource::with_reader(plan, reader);

        let error = source
            .next_event()
            .expect_err("missing kprobe symbol should fail startup");

        assert!(matches!(error, SourceError::Unsupported(_)));
        assert!(error.to_string().contains("missing kprobe:handle_mm_fault"));
    }
}
