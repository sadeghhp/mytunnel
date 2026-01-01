//! Metrics and observability
//!
//! Prometheus-compatible metrics with atomic counters for the hot path.

mod api;
mod counters;
mod exporter;

pub use api::start_api_server;
pub use counters::*;
pub use exporter::init_metrics;

