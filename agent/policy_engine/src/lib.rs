#![forbid(unsafe_code)]

//! Policy evaluation and conflict resolution for the AegisAI Runtime control loop.
//!
//! This crate converts scenario-specific thresholds, workload tags, and safety
//! guardrails into bounded action plans. It does not execute system actions.

mod config;
mod engine;
mod model;
mod scenarios;

pub use config::{SafetyConfig, ScenarioActions, ScenarioPolicy, TriggerThresholds};
pub use engine::PolicyEngine;
pub use model::{
    Action, ActionPlan, EventContext, FeatureWindow, PinStrategy, PolicyContext, ScenarioKind,
    WorkloadProfile, WorkloadTag,
};
