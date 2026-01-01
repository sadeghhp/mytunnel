//! QUIC server listener
//!
//! High-performance QUIC listener with SO_REUSEPORT for multi-core scaling.

use anyhow::{Context, Result};
use quinn::{Endpoint, ServerConfig, TransportConfig, VarInt};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::watch;
use tracing::{debug, info, warn};

use crate::config::Config;
use crate::connection::{ConnectionManager, ConnectionManagerConfig};
use crate::pool::BufferPool;

use super::acceptor::ConnectionHandler;

/// QUIC tunnel server
pub struct Server {
    /// QUIC endpoint
    endpoint: Endpoint,
    /// Server configuration
    config: Arc<Config>,
    /// Connection manager
    conn_manager: Arc<ConnectionManager>,
    /// Buffer pool
    buffer_pool: BufferPool,
    /// Shutdown signal
    shutdown_rx: watch::Receiver<bool>,
    shutdown_tx: watch::Sender<bool>,
}

impl Server {
    /// Create a new server instance
    pub async fn new(config: Arc<Config>) -> Result<Self> {
        // Initialize buffer pool
        let buffer_pool = BufferPool::new(
            config.pool.buffer_count_4k,
            config.pool.buffer_count_16k,
            config.pool.buffer_count_64k,
        );
        info!(
            small = config.pool.buffer_count_4k,
            medium = config.pool.buffer_count_16k,
            large = config.pool.buffer_count_64k,
            "Buffer pool initialized"
        );

        // Initialize connection manager
        let conn_manager = ConnectionManager::new(ConnectionManagerConfig {
            max_connections: config.pool.connection_slots,
            idle_timeout: Duration::from_secs(config.quic.idle_timeout_secs),
        });

        // Load or generate TLS configuration
        let server_config = build_server_config(&config).await?;

        // Create UDP socket with optimizations
        let socket = crate::util::create_udp_socket(config.server.bind_addr, true)?;

        // Create QUIC endpoint
        let runtime = quinn::default_runtime()
            .ok_or_else(|| anyhow::anyhow!("No async runtime found"))?;
        
        let endpoint = Endpoint::new(
            quinn::EndpointConfig::default(),
            Some(server_config),
            socket,
            runtime,
        )?;

        let (shutdown_tx, shutdown_rx) = watch::channel(false);

        Ok(Self {
            endpoint,
            config,
            conn_manager,
            buffer_pool,
            shutdown_rx,
            shutdown_tx,
        })
    }

    /// Run the server (main accept loop)
    pub async fn run(&self) -> Result<()> {
        info!(
            bind_addr = %self.config.server.bind_addr,
            "Server accepting connections"
        );

        // Start idle connection cleanup task
        let conn_manager = self.conn_manager.clone();
        let cleanup_interval = Duration::from_secs(self.config.quic.idle_timeout_secs / 2);
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(cleanup_interval);
            loop {
                interval.tick().await;
                conn_manager.cleanup_idle();
            }
        });

        let mut shutdown_rx = self.shutdown_rx.clone();

        loop {
            tokio::select! {
                // Accept new connections
                incoming = self.endpoint.accept() => {
                    match incoming {
                        Some(incoming) => {
                            // Check capacity
                            if self.conn_manager.is_full() {
                                warn!("Connection rejected: at capacity");
                                // Connection will be dropped
                                continue;
                            }

                            // Spawn handler for this connection
                            let handler = ConnectionHandler::new(
                                self.conn_manager.clone(),
                                self.buffer_pool.clone(),
                                self.config.clone(),
                            );

                            tokio::spawn(async move {
                                if let Err(e) = handler.handle(incoming).await {
                                    debug!(error = %e, "Connection error");
                                }
                            });
                        }
                        None => {
                            // Endpoint closed
                            break;
                        }
                    }
                }
                // Shutdown signal
                _ = shutdown_rx.changed() => {
                    if *shutdown_rx.borrow() {
                        info!("Shutdown signal received");
                        break;
                    }
                }
            }
        }

        Ok(())
    }

    /// Gracefully shutdown the server
    pub async fn shutdown(&self) {
        info!("Initiating graceful shutdown");

        // Signal shutdown
        let _ = self.shutdown_tx.send(true);

        // Signal all connections
        self.conn_manager.signal_shutdown();

        // Drain connections (wait up to 30 seconds)
        self.conn_manager.drain(Duration::from_secs(30)).await;

        // Close endpoint
        self.endpoint.close(VarInt::from_u32(0), b"server shutdown");

        info!("Server shutdown complete");
    }
}

/// Build QUIC server configuration
async fn build_server_config(config: &Config) -> Result<ServerConfig> {
    // Load or generate certificates
    let (certs, key) = load_or_generate_certs(config).await?;

    // Build rustls config
    let mut rustls_config = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .context("Failed to build TLS config")?;

    // Enable ALPN
    rustls_config.alpn_protocols = vec![b"mytunnel".to_vec(), b"h3".to_vec()];

    // Create quinn server config
    let mut server_config = ServerConfig::with_crypto(Arc::new(
        quinn::crypto::rustls::QuicServerConfig::try_from(rustls_config)?,
    ));

    // Configure transport
    let mut transport = TransportConfig::default();

    // Connection settings
    transport.max_concurrent_bidi_streams(VarInt::from_u32(config.quic.max_streams_per_conn));
    transport.max_concurrent_uni_streams(VarInt::from_u32(config.quic.max_streams_per_conn));
    transport.max_idle_timeout(Some(
        Duration::from_secs(config.quic.idle_timeout_secs)
            .try_into()
            .unwrap(),
    ));

    // Enable datagrams for UDP relay
    transport.datagram_receive_buffer_size(Some(65536));
    transport.datagram_send_buffer_size(65536);

    // Performance settings
    transport.initial_rtt(Duration::from_millis(100));
    transport.send_window(8 * 1024 * 1024); // 8MB
    transport.receive_window(VarInt::from_u32(8 * 1024 * 1024));
    transport.stream_receive_window(VarInt::from_u32(2 * 1024 * 1024));

    // Keep-alive for NAT traversal
    transport.keep_alive_interval(Some(Duration::from_secs(15)));

    // Apply transport config
    server_config.transport_config(Arc::new(transport));

    // Enable migration for mobile clients
    server_config.migration(true);

    Ok(server_config)
}

/// Load certificates from files or generate self-signed
async fn load_or_generate_certs(
    config: &Config,
) -> Result<(Vec<CertificateDer<'static>>, PrivateKeyDer<'static>)> {
    let cert_path = std::path::Path::new(&config.tls.cert_path);
    let key_path = std::path::Path::new(&config.tls.key_path);

    if cert_path.exists() && key_path.exists() {
        // Load from files
        info!(cert = %config.tls.cert_path, key = %config.tls.key_path, "Loading TLS certificates");

        let cert_pem = tokio::fs::read(&config.tls.cert_path)
            .await
            .context("Failed to read certificate file")?;
        let key_pem = tokio::fs::read(&config.tls.key_path)
            .await
            .context("Failed to read key file")?;

        let certs: Vec<CertificateDer<'static>> = rustls_pemfile::certs(&mut cert_pem.as_slice())
            .collect::<Result<Vec<_>, _>>()
            .context("Failed to parse certificates")?;

        let key = rustls_pemfile::private_key(&mut key_pem.as_slice())
            .context("Failed to parse private key")?
            .ok_or_else(|| anyhow::anyhow!("No private key found in file"))?;

        Ok((certs, key))
    } else if config.tls.auto_generate {
        // Generate self-signed certificate
        warn!("Generating self-signed certificate (not for production use)");

        let cert = rcgen::generate_simple_self_signed(vec!["localhost".to_string()])
            .context("Failed to generate self-signed certificate")?;

        let cert_der = CertificateDer::from(cert.cert);
        let key_der = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(cert.key_pair.serialize_der()));

        Ok((vec![cert_der], key_der))
    } else {
        anyhow::bail!(
            "TLS certificate not found at {} and auto_generate is disabled",
            config.tls.cert_path
        )
    }
}

