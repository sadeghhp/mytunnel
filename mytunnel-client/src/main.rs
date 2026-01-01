//! MyTunnel Client - Entry Point
//!
//! CLI application for running the tunnel client.

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::signal;
use tracing::{error, info};

use mytunnel_client::{Config, TunnelClient, VERSION};

/// MyTunnel Client - QUIC tunnel with SOCKS5/HTTP proxy
#[derive(Parser)]
#[command(name = "mytunnel-client")]
#[command(version = VERSION)]
#[command(about = "QUIC-based tunnel client with SOCKS5 and HTTP proxy")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Run the tunnel client
    Run {
        /// Path to configuration file
        #[arg(short, long, default_value = "client-config.toml")]
        config: PathBuf,
    },
    /// Test connection to the server
    TestConnection {
        /// Path to configuration file
        #[arg(short, long, default_value = "client-config.toml")]
        config: PathBuf,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    // Install the ring crypto provider for rustls
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("Failed to install rustls crypto provider");

    let cli = Cli::parse();

    match cli.command {
        Commands::Run { config } => run_client(config).await,
        Commands::TestConnection { config } => test_connection(config).await,
    }
}

async fn run_client(config_path: PathBuf) -> Result<()> {
    // Load configuration
    let config = Config::load(&config_path)
        .with_context(|| format!("Failed to load config from {:?}", config_path))?;

    // Initialize tracing
    init_tracing(&config.logging)?;

    info!(
        version = VERSION,
        config_path = ?config_path,
        "Starting MyTunnel Client"
    );

    let config = Arc::new(config);

    // Create and start the tunnel client
    let client = TunnelClient::new(config.clone()).await?;

    info!(
        server = %config.server.address,
        socks5 = %config.proxy.socks5_bind,
        http = %config.proxy.http_bind,
        "Client started"
    );

    // Run client with graceful shutdown
    tokio::select! {
        result = client.run() => {
            if let Err(e) = result {
                error!(error = %e, "Client error");
                return Err(e);
            }
        }
        _ = shutdown_signal() => {
            info!("Shutdown signal received");
            client.shutdown().await;
        }
    }

    info!("Client stopped");
    Ok(())
}

async fn test_connection(config_path: PathBuf) -> Result<()> {
    // Load configuration
    let config = Config::load(&config_path)
        .with_context(|| format!("Failed to load config from {:?}", config_path))?;

    // Initialize simple tracing
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    info!(
        server = %config.server.address,
        "Testing connection to server"
    );

    let config = Arc::new(config);

    // Try to establish connection
    match TunnelClient::test_connection(config.clone()).await {
        Ok(()) => {
            info!("Connection test successful!");
            Ok(())
        }
        Err(e) => {
            error!(error = %e, "Connection test failed");
            Err(e)
        }
    }
}

fn init_tracing(logging_config: &mytunnel_client::config::LoggingConfig) -> Result<()> {
    use tracing_subscriber::{fmt, prelude::*, EnvFilter};

    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(&logging_config.level));

    let subscriber = tracing_subscriber::registry().with(filter);

    if logging_config.format == "json" {
        subscriber.with(fmt::layer().json()).init();
    } else {
        subscriber.with(fmt::layer()).init();
    }

    Ok(())
}

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

