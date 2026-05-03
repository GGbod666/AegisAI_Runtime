#![forbid(unsafe_code)]

mod metadata;
mod runtime_loop;
mod source;

pub use metadata::{
    enrich_source_event, MetadataError, MetadataProvider, NoopMetadataProvider, ProcessMetadata,
    ProcfsMetadataProvider, StaticMetadataProvider,
};
pub use runtime_loop::{
    RuntimeLoop, RuntimeLoopConfig, RuntimeLoopError, RuntimeRunSummary, SignalObservationSummary,
    ToolCallLifecycleSummary,
};
pub use source::{
    BpfTracePipe, BpfTraceProbeDriver, DriverBackedProbeEventReader, EventSource, LinuxProbeDriver,
    LinuxProbeHost, LinuxProbePlan, LinuxProbeSource, MockEventSource, PlannedProbe,
    PreflightLinuxProbeDriver, ProbeAttachment, ProbeAttachmentStatus, ProbeEventReader,
    ProbeReaderConfig, ProbeReaderShutdown, ProbeReaderStartup, ProcfsSchedstatProbeDriver,
    ProcfsSchedstatSampler, ProcfsSchedstatSnapshot, ProcfsTargetSelectors, RealLinuxProbeDriver,
    SourceError, SourceEvent, StaticProbeEventReader, SystemBpfTracePipe, SystemLinuxProbeHost,
    SystemProcfsSchedstatSampler, UnavailableLinuxProbeDriver, UnsupportedProbeEventReader,
};
