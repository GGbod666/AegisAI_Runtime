use std::collections::{BTreeSet, HashMap};
use std::fmt;

use crate::SourceEvent;
use runtime_orchestrator::Event;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProcessMetadata {
    pub process_name: Option<String>,
    pub cmdline: Option<String>,
    pub cgroup: Option<String>,
    pub tag_markers: BTreeSet<String>,
    pub parent_pid: Option<u32>,
    pub parent_process_name: Option<String>,
    pub parent_cmdline: Option<String>,
}

impl ProcessMetadata {
    pub fn new(process_name: impl Into<String>) -> Self {
        Self {
            process_name: Some(process_name.into()),
            ..Self::default()
        }
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
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MetadataError {
    Unsupported(String),
    Invalid(String),
}

impl fmt::Display for MetadataError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unsupported(message) => write!(f, "{message}"),
            Self::Invalid(message) => write!(f, "{message}"),
        }
    }
}

impl std::error::Error for MetadataError {}

pub trait MetadataProvider {
    fn provider_name(&self) -> &str;

    fn snapshot_process(&mut self, pid: u32) -> Result<Option<ProcessMetadata>, MetadataError>;
}

#[derive(Default)]
pub struct NoopMetadataProvider;

impl MetadataProvider for NoopMetadataProvider {
    fn provider_name(&self) -> &str {
        "noop"
    }

    fn snapshot_process(&mut self, _pid: u32) -> Result<Option<ProcessMetadata>, MetadataError> {
        Ok(None)
    }
}

#[derive(Default)]
pub struct StaticMetadataProvider {
    snapshots: HashMap<u32, ProcessMetadata>,
}

impl StaticMetadataProvider {
    pub fn new(snapshots: HashMap<u32, ProcessMetadata>) -> Self {
        Self { snapshots }
    }

    pub fn insert(&mut self, pid: u32, metadata: ProcessMetadata) {
        self.snapshots.insert(pid, metadata);
    }

    pub fn demo() -> Self {
        let mut snapshots = HashMap::new();
        snapshots.insert(
            4_242,
            ProcessMetadata::new("ollama")
                .with_cmdline("ollama serve")
                .with_cgroup("/aegisai/inference")
                .with_tag_marker("interactive"),
        );
        snapshots.insert(
            5_151,
            ProcessMetadata::new("python")
                .with_cmdline("python tool-executor retrieval-worker")
                .with_parent_pid(4_242)
                .with_parent_process_name("ollama")
                .with_parent_cmdline("ollama serve")
                .with_tag_marker("tool-executor"),
        );
        Self { snapshots }
    }
}

impl MetadataProvider for StaticMetadataProvider {
    fn provider_name(&self) -> &str {
        "static"
    }

    fn snapshot_process(&mut self, pid: u32) -> Result<Option<ProcessMetadata>, MetadataError> {
        Ok(self.snapshots.get(&pid).cloned())
    }
}

#[cfg(target_os = "linux")]
#[derive(Default)]
pub struct ProcfsMetadataProvider {
    cache: HashMap<u32, ProcessMetadata>,
}

#[cfg(target_os = "linux")]
impl MetadataProvider for ProcfsMetadataProvider {
    fn provider_name(&self) -> &str {
        "procfs"
    }

    fn snapshot_process(&mut self, pid: u32) -> Result<Option<ProcessMetadata>, MetadataError> {
        if let Some(metadata) = self.cache.get(&pid) {
            return Ok(Some(metadata.clone()));
        }

        let Some(metadata) = read_procfs_metadata(pid)? else {
            return Ok(None);
        };
        self.cache.insert(pid, metadata.clone());
        Ok(Some(metadata))
    }
}

#[cfg(target_os = "linux")]
fn read_procfs_metadata(pid: u32) -> Result<Option<ProcessMetadata>, MetadataError> {
    use std::fs;

    let root = format!("/proc/{pid}");
    let comm_path = format!("{root}/comm");
    let status_path = format!("{root}/status");
    let cmdline_path = format!("{root}/cmdline");
    let cgroup_path = format!("{root}/cgroup");

    let process_name = match fs::read_to_string(&comm_path) {
        Ok(value) => value.trim().to_string(),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(MetadataError::Invalid(format!(
                "failed to read {comm_path}: {error}"
            )));
        }
    };

    let cmdline = match fs::read(&cmdline_path) {
        Ok(bytes) => bytes
            .split(|byte| *byte == 0)
            .filter(|item| !item.is_empty())
            .map(|item| String::from_utf8_lossy(item).to_string())
            .collect::<Vec<_>>()
            .join(" "),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => {
            return Err(MetadataError::Invalid(format!(
                "failed to read {cmdline_path}: {error}"
            )));
        }
    };

    let cgroup = match fs::read_to_string(&cgroup_path) {
        Ok(raw) => raw
            .lines()
            .find_map(|line| line.rsplit(':').next().map(str::trim))
            .filter(|value| !value.is_empty())
            .map(str::to_string),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => {
            return Err(MetadataError::Invalid(format!(
                "failed to read {cgroup_path}: {error}"
            )));
        }
    };

    let status = fs::read_to_string(&status_path).map_err(|error| {
        MetadataError::Invalid(format!("failed to read {status_path}: {error}"))
    })?;

    let parent_pid = parse_status_value(&status, "PPid:")
        .and_then(|value| value.parse::<u32>().ok())
        .filter(|value| *value > 0);

    let (parent_process_name, parent_cmdline) = if let Some(parent_pid) = parent_pid {
        read_parent_metadata(parent_pid)?
    } else {
        (None, None)
    };

    let mut metadata = ProcessMetadata::new(process_name)
        .with_cmdline(cmdline)
        .with_tag_markers(BTreeSet::<String>::new());
    metadata.cgroup = cgroup;
    metadata.parent_pid = parent_pid;
    metadata.parent_process_name = parent_process_name;
    metadata.parent_cmdline = parent_cmdline;

    Ok(Some(metadata))
}

#[cfg(target_os = "linux")]
fn read_parent_metadata(pid: u32) -> Result<(Option<String>, Option<String>), MetadataError> {
    use std::fs;

    let root = format!("/proc/{pid}");
    let comm = fs::read_to_string(format!("{root}/comm"))
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    let cmdline = fs::read(format!("{root}/cmdline"))
        .ok()
        .map(|bytes| {
            bytes
                .split(|byte| *byte == 0)
                .filter(|item| !item.is_empty())
                .map(|item| String::from_utf8_lossy(item).to_string())
                .collect::<Vec<_>>()
                .join(" ")
        })
        .filter(|value| !value.is_empty());

    Ok((comm, cmdline))
}

#[cfg(target_os = "linux")]
fn parse_status_value<'a>(status: &'a str, key: &str) -> Option<&'a str> {
    status
        .lines()
        .find_map(|line| line.strip_prefix(key))
        .map(str::trim)
}

#[cfg(not(target_os = "linux"))]
#[derive(Default)]
pub struct ProcfsMetadataProvider;

#[cfg(not(target_os = "linux"))]
impl MetadataProvider for ProcfsMetadataProvider {
    fn provider_name(&self) -> &str {
        "procfs"
    }

    fn snapshot_process(&mut self, _pid: u32) -> Result<Option<ProcessMetadata>, MetadataError> {
        Err(MetadataError::Unsupported(
            "procfs metadata provider is only available on Linux".to_string(),
        ))
    }
}

pub fn enrich_source_event<P: MetadataProvider>(
    source: SourceEvent,
    provider: &mut P,
) -> Result<Event, MetadataError> {
    let metadata = if source.needs_enrichment() {
        provider.snapshot_process(source.pid)?
    } else {
        None
    };

    let process_name = source
        .process_name
        .or_else(|| metadata.as_ref().and_then(|item| item.process_name.clone()))
        .ok_or_else(|| {
            MetadataError::Invalid(format!(
                "process_name is required for pid {} after enrichment via {}",
                source.pid,
                provider.provider_name()
            ))
        })?;

    let cmdline = source
        .cmdline
        .or_else(|| metadata.as_ref().and_then(|item| item.cmdline.clone()))
        .unwrap_or_default();

    let cgroup = source
        .cgroup
        .or_else(|| metadata.as_ref().and_then(|item| item.cgroup.clone()));

    let parent_pid = source
        .parent_pid
        .or_else(|| metadata.as_ref().and_then(|item| item.parent_pid));

    let parent_process_name = source.parent_process_name.or_else(|| {
        metadata
            .as_ref()
            .and_then(|item| item.parent_process_name.clone())
    });

    let parent_cmdline = source.parent_cmdline.or_else(|| {
        metadata
            .as_ref()
            .and_then(|item| item.parent_cmdline.clone())
    });

    let mut tag_markers = source.tag_markers;
    if let Some(metadata) = metadata {
        tag_markers.extend(metadata.tag_markers);
    }

    let mut event = Event::new(
        source.timestamp_ms,
        source.pid,
        process_name,
        source.signal,
        source.value,
    )
    .with_cmdline(cmdline)
    .with_tag_markers(tag_markers);

    if let Some(tid) = source.tid {
        event.tid = Some(tid);
    }
    if let Some(cgroup) = cgroup {
        event = event.with_cgroup(cgroup);
    }
    if let Some(parent_pid) = parent_pid {
        event = event.with_parent_pid(parent_pid);
    }
    if let Some(parent_process_name) = parent_process_name {
        event = event.with_parent_process_name(parent_process_name);
    }
    if let Some(parent_cmdline) = parent_cmdline {
        event = event.with_parent_cmdline(parent_cmdline);
    }

    Ok(event)
}

#[cfg(test)]
mod tests {
    use crate::{enrich_source_event, MetadataProvider, NoopMetadataProvider};
    use runtime_orchestrator::SignalKind;

    use super::{MetadataError, ProcessMetadata, StaticMetadataProvider};
    use std::collections::HashMap;

    #[test]
    fn static_provider_fills_missing_fields() {
        let mut provider = StaticMetadataProvider::new(HashMap::from([(
            9,
            ProcessMetadata::new("ollama").with_cmdline("ollama serve"),
        )]));

        let event = enrich_source_event(
            crate::SourceEvent::new(1_000, 9, SignalKind::RunQueueDelay, 1_500),
            &mut provider,
        )
        .expect("event should enrich");

        assert_eq!(event.process_name, "ollama");
        assert_eq!(event.cmdline, "ollama serve");
    }

    #[test]
    fn missing_process_name_is_rejected() {
        let mut provider = NoopMetadataProvider;
        let error = enrich_source_event(
            crate::SourceEvent::new(1_000, 9, SignalKind::RunQueueDelay, 1_500),
            &mut provider,
        )
        .expect_err("missing name should fail");

        assert!(matches!(error, MetadataError::Invalid(_)));
    }

    #[test]
    fn noop_provider_returns_none() {
        let mut provider = NoopMetadataProvider;
        assert!(provider
            .snapshot_process(42)
            .expect("noop provider should succeed")
            .is_none());
    }
}
