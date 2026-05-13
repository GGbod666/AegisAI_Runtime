pub mod config;
pub mod model;
mod runtime_orchestrator;

pub use aegisai_metrics::{
    Measurement, MetricKind, MetricRecord, MetricTrace, MetricTrend, MetricsConfig,
    MetricsConfigError, MetricsRecorder, RecordInput, ScenarioStats, SideEffect, TraceKind,
};
pub use config::{
    AwarenessConfig, ClassifierConfig, ConfigError, ProcessRule, RuntimeConfig,
    RuntimeConfigProfile, RuntimeOrchestratorConfig, SafetyConfig, ScenarioActions, ScenarioPolicy,
    TriggerThresholds,
};
pub use model::{
    Action, ActionPlan, AppliedAction, AppliedActionState, Event, EventContext, FeatureWindow,
    LatencySensitivity, OrchestrationOutcome, OwnershipScope, PinStrategy, PolicyContext,
    ScenarioKind, SignalKind, StageLabel, WorkloadClass, WorkloadProfile, WorkloadTag,
};
pub use runtime_orchestrator::{
    RuntimeOrchestrator, RuntimeOrchestratorInitError, RuntimeOrchestratorLoadError,
};
