//! Metrics and observability
//!
//! Prometheus-compatible metrics with atomic counters for the hot path.

mod counters;
mod exporter;

pub use counters::*;
pub use exporter::init_metrics;

