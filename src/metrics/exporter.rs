//! Prometheus metrics exporter
//!
//! HTTP endpoint for Prometheus scraping.

use anyhow::Result;
use metrics::{describe_counter, describe_gauge, gauge, counter};
use metrics_exporter_prometheus::PrometheusBuilder;
use std::net::SocketAddr;
use tokio::task::JoinHandle;

use crate::config::MetricsConfig;
use super::counters::METRICS;

/// Initialize the Prometheus metrics exporter
pub fn init_metrics(config: &MetricsConfig) -> Result<()> {
    // Register metric descriptions
    describe_counter!("mytunnel_connections_total", "Total connections received");
    describe_gauge!("mytunnel_connections_active", "Currently active connections");
    describe_counter!("mytunnel_connections_failed", "Failed connection attempts");
    describe_counter!("mytunnel_bytes_received", "Total bytes received");
    describe_counter!("mytunnel_bytes_sent", "Total bytes sent");
    describe_counter!("mytunnel_packets_received", "Total packets received");
    describe_counter!("mytunnel_packets_sent", "Total packets sent");
    describe_counter!("mytunnel_streams_opened", "Total streams opened");
    describe_counter!("mytunnel_streams_closed", "Total streams closed");
    describe_counter!("mytunnel_datagrams_received", "Total datagrams received");
    describe_counter!("mytunnel_datagrams_sent", "Total datagrams sent");
    describe_counter!("mytunnel_errors_total", "Total errors");
    describe_counter!("mytunnel_timeouts_total", "Total timeouts");

    // Build and install the Prometheus exporter
    PrometheusBuilder::new()
        .with_http_listener(config.bind_addr)
        .install()?;

    // Start background task to sync atomic counters to metrics crate
    tokio::spawn(sync_metrics_task());

    Ok(())
}

/// Background task that periodically syncs our atomic counters to the metrics crate
async fn sync_metrics_task() {
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(1));

    let mut last_snapshot = METRICS.snapshot();

    loop {
        interval.tick().await;

        let snapshot = METRICS.snapshot();

        // Update counters with deltas
        let conn_delta = snapshot.connections_total.saturating_sub(last_snapshot.connections_total);
        if conn_delta > 0 {
            counter!("mytunnel_connections_total").increment(conn_delta);
        }

        gauge!("mytunnel_connections_active").set(snapshot.connections_active as f64);

        let failed_delta = snapshot.connections_failed.saturating_sub(last_snapshot.connections_failed);
        if failed_delta > 0 {
            counter!("mytunnel_connections_failed").increment(failed_delta);
        }

        let rx_delta = snapshot.bytes_received.saturating_sub(last_snapshot.bytes_received);
        if rx_delta > 0 {
            counter!("mytunnel_bytes_received").increment(rx_delta);
        }

        let tx_delta = snapshot.bytes_sent.saturating_sub(last_snapshot.bytes_sent);
        if tx_delta > 0 {
            counter!("mytunnel_bytes_sent").increment(tx_delta);
        }

        let pkt_rx_delta = snapshot.packets_received.saturating_sub(last_snapshot.packets_received);
        if pkt_rx_delta > 0 {
            counter!("mytunnel_packets_received").increment(pkt_rx_delta);
        }

        let pkt_tx_delta = snapshot.packets_sent.saturating_sub(last_snapshot.packets_sent);
        if pkt_tx_delta > 0 {
            counter!("mytunnel_packets_sent").increment(pkt_tx_delta);
        }

        let streams_opened_delta = snapshot.streams_opened.saturating_sub(last_snapshot.streams_opened);
        if streams_opened_delta > 0 {
            counter!("mytunnel_streams_opened").increment(streams_opened_delta);
        }

        let streams_closed_delta = snapshot.streams_closed.saturating_sub(last_snapshot.streams_closed);
        if streams_closed_delta > 0 {
            counter!("mytunnel_streams_closed").increment(streams_closed_delta);
        }

        let dg_rx_delta = snapshot.datagrams_received.saturating_sub(last_snapshot.datagrams_received);
        if dg_rx_delta > 0 {
            counter!("mytunnel_datagrams_received").increment(dg_rx_delta);
        }

        let dg_tx_delta = snapshot.datagrams_sent.saturating_sub(last_snapshot.datagrams_sent);
        if dg_tx_delta > 0 {
            counter!("mytunnel_datagrams_sent").increment(dg_tx_delta);
        }

        let errors_delta = snapshot.errors_total.saturating_sub(last_snapshot.errors_total);
        if errors_delta > 0 {
            counter!("mytunnel_errors_total").increment(errors_delta);
        }

        let timeouts_delta = snapshot.timeouts_total.saturating_sub(last_snapshot.timeouts_total);
        if timeouts_delta > 0 {
            counter!("mytunnel_timeouts_total").increment(timeouts_delta);
        }

        last_snapshot = snapshot;
    }
}

/// Start a simple HTTP server for health checks and metrics
#[allow(dead_code)]
pub fn start_health_server(addr: SocketAddr) -> JoinHandle<()> {
    tokio::spawn(async move {
        // The Prometheus exporter already provides /metrics
        // This could be extended to add /health and /ready endpoints
        tracing::info!(%addr, "Health server running (metrics at /metrics)");
        
        // Keep task alive - actual serving is done by PrometheusBuilder
        std::future::pending::<()>().await;
    })
}

