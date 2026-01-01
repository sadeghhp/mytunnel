//! MyTunnel Server - Entry Point
//!
//! High-performance QUIC-based tunnel server with zero-copy forwarding.

use anyhow::{Context, Result};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::signal;
use tracing::{error, info};

use mytunnel_server::{Config, Server, VERSION};

/// Application entry point
#[tokio::main]
async fn main() -> Result<()> {
    // Parse command line arguments
    let config_path = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("config.toml"));

    // Load configuration
    let config = Config::load(&config_path)
        .with_context(|| format!("Failed to load config from {:?}", config_path))?;

    // Initialize tracing/logging
    mytunnel_server::util::init_tracing(&config.logging)?;

    info!(
        version = VERSION,
        config_path = ?config_path,
        "Starting MyTunnel Server"
    );

    // Initialize metrics if enabled
    if config.metrics.enabled {
        mytunnel_server::metrics::init_metrics(&config.metrics)?;
        info!(
            bind_addr = %config.metrics.bind_addr,
            "Metrics endpoint started"
        );
    }

    // Create and start the server
    let config = Arc::new(config);
    let server = Server::new(config.clone()).await?;

    info!(
        bind_addr = %config.server.bind_addr,
        workers = config.server.effective_workers(),
        "Server listening"
    );

    // Run server with graceful shutdown
    tokio::select! {
        result = server.run() => {
            if let Err(e) = result {
                error!(error = %e, "Server error");
                return Err(e);
            }
        }
        _ = shutdown_signal() => {
            info!("Shutdown signal received, draining connections...");
            server.shutdown().await;
        }
    }

    info!("Server stopped");
    Ok(())
}

/// Wait for shutdown signal (Ctrl+C or SIGTERM)
async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("Failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("Failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}

