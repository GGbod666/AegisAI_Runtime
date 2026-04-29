#![forbid(unsafe_code)]

//! Windowed event aggregation for the AegisAI Runtime control loop.
//!
//! The collector turns a stream of low-level probe events into stable feature
//! windows that downstream classifier and policy modules can consume.

mod collector;
mod config;
mod event;
mod summary;

pub use collector::{AggregationTarget, Collector, CollectorError, CollectorStats, FeatureWindow};
pub use config::{AggregationScope, CollectorConfig, NoiseFilter};
pub use event::{Event, EventConversionError, EventKind, ProbeSource, TimestampMicros};
pub use summary::{CounterSummary, ValueSummary};
