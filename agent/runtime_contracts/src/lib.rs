#![forbid(unsafe_code)]

//! Shared runtime contracts for the AegisAI Runtime control loop.
//!
//! This crate holds the common domain model used across the runtime
//! orchestrator, policy engine, and actuator so the mainline architecture can
//! evolve without type drift.

mod config;
mod model;

pub use config::{SafetyConfig, ScenarioActions, ScenarioPolicy, TriggerThresholds};
pub use model::{
    Action, ActionPlan, AppliedAction, AppliedActionState, Event, EventContext, FeatureWindow,
    LatencySensitivity, OwnershipScope, PinStrategy, PolicyContext, ScenarioKind, SignalKind,
    StageLabel, WorkloadClass, WorkloadProfile, WorkloadTag,
};
