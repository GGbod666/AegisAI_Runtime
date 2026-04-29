use std::collections::BTreeMap;

pub use aegisai_metrics::MetricRecord;
pub use aegisai_runtime_contracts::{
    Action, ActionPlan, AppliedAction, AppliedActionState, Event, EventContext, FeatureWindow,
    LatencySensitivity, OwnershipScope, PinStrategy, PolicyContext, ScenarioKind, SignalKind,
    StageLabel, WorkloadClass, WorkloadProfile, WorkloadTag,
};

#[derive(Clone, Debug, PartialEq)]
pub struct OrchestrationOutcome {
    pub profile: WorkloadProfile,
    pub feature_windows: BTreeMap<ScenarioKind, FeatureWindow>,
    pub applied_actions: Vec<AppliedAction>,
    pub rollbacks: Vec<AppliedAction>,
    pub metric_record: MetricRecord,
}
