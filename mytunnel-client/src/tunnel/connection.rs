//! QUIC connection management
//!
//! Handles establishing and maintaining the QUIC connection to the server.

use anyhow::{Context, Result};
use bytes::Bytes;
use parking_lot::RwLock;
use quinn::{Connection, Endpoint, RecvStream, SendStream};
use rustls::pki_types::ServerName;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast;
use tracing::{debug, error, info, warn};

use crate::config::Config;
use crate::proxy::{HttpProxy, Socks5Proxy};

/// Tunnel client that manages the QUIC connection and local proxies
pub struct TunnelClient {
    config: Arc<Config>,
    endpoint: Endpoint,
    connection: Arc<RwLock<Option<Connection>>>,
    shutdown_tx: broadcast::Sender<()>,
}

impl TunnelClient {
    /// Create a new tunnel client
    pub async fn new(config: Arc<Config>) -> Result<Self> {
        let endpoint = create_client_endpoint(&config)?;

        let (shutdown_tx, _) = broadcast::channel(1);

        Ok(Self {
            config,
            endpoint,
            connection: Arc::new(RwLock::new(None)),
            shutdown_tx,
        })
    }

    /// Test connection to the server
    pub async fn test_connection(config: Arc<Config>) -> Result<()> {
        let endpoint = create_client_endpoint(&config)?;

        // Resolve server address
        let server_addr = resolve_address(&config.server.address).await?;
        let server_name = config.server.get_server_name().to_string();

        info!(addr = %server_addr, name = %server_name, "Connecting to server");

        // Connect to server
        let connection = endpoint
            .connect(server_addr, &server_name)?
            .await
            .context("Failed to establish QUIC connection")?;

        info!(
            "Connected! Remote address: {}, Protocol: {:?}",
            connection.remote_address(),
            connection.handshake_data()
                .and_then(|h| h.downcast::<quinn::crypto::rustls::HandshakeData>().ok())
                .and_then(|h| h.protocol.map(|p| String::from_utf8_lossy(&p).to_string()))
        );

        // Close connection gracefully
        connection.close(quinn::VarInt::from_u32(0), b"test complete");

        Ok(())
    }

    /// Connect to the server
    async fn connect(&self) -> Result<Connection> {
        let server_addr = resolve_address(&self.config.server.address).await?;
        let server_name = self.config.server.get_server_name().to_string();

        debug!(addr = %server_addr, name = %server_name, "Connecting to server");

        let connection = self
            .endpoint
            .connect(server_addr, &server_name)?
            .await
            .context("Failed to establish QUIC connection")?;

        info!(addr = %connection.remote_address(), "Connected to server");

        Ok(connection)
    }

    /// Get or establish connection
    pub async fn get_connection(&self) -> Result<Connection> {
        // Check existing connection
        {
            let conn = self.connection.read();
            if let Some(ref c) = *conn {
                if !c.close_reason().is_some() {
                    return Ok(c.clone());
                }
            }
        }

        // Need to establish new connection
        let new_conn = self.connect().await?;

        {
            let mut conn = self.connection.write();
            *conn = Some(new_conn.clone());
        }

        Ok(new_conn)
    }

    /// Open a bidirectional stream for TCP tunneling
    pub async fn open_stream(&self) -> Result<(SendStream, RecvStream)> {
        let conn = self.get_connection().await?;
        let (send, recv) = conn.open_bi().await.context("Failed to open stream")?;
        Ok((send, recv))
    }

    /// Send a datagram for UDP relay
    pub async fn send_datagram(&self, data: Bytes) -> Result<()> {
        let conn = self.get_connection().await?;
        conn.send_datagram(data)
            .context("Failed to send datagram")?;
        Ok(())
    }

    /// Receive datagrams (for UDP responses)
    pub async fn recv_datagram(&self) -> Result<Bytes> {
        let conn = self.get_connection().await?;
        let data = conn
            .read_datagram()
            .await
            .context("Failed to receive datagram")?;
        Ok(data)
    }

    /// Run the tunnel client with local proxy servers
    pub async fn run(&self) -> Result<()> {
        // Establish initial connection
        let conn = self.connect().await?;
        {
            let mut c = self.connection.write();
            *c = Some(conn);
        }

        // Create shared client reference for proxies
        let client = Arc::new(TunnelClientHandle {
            connection: self.connection.clone(),
            config: self.config.clone(),
            endpoint: self.endpoint.clone(),
        });

        let mut handles = Vec::new();

        // Start SOCKS5 proxy if enabled
        if self.config.proxy.socks5_enabled {
            let socks5 = Socks5Proxy::new(client.clone(), self.config.proxy.socks5_bind);
            let mut shutdown_rx = self.shutdown_tx.subscribe();

            handles.push(tokio::spawn(async move {
                tokio::select! {
                    result = socks5.run() => {
                        if let Err(e) = result {
                            error!(error = %e, "SOCKS5 proxy error");
                        }
                    }
                    _ = shutdown_rx.recv() => {
                        debug!("SOCKS5 proxy shutting down");
                    }
                }
            }));

            info!(bind = %self.config.proxy.socks5_bind, "SOCKS5 proxy started");
        }

        // Start HTTP proxy if enabled
        if self.config.proxy.http_enabled {
            let http = HttpProxy::new(client.clone(), self.config.proxy.http_bind);
            let mut shutdown_rx = self.shutdown_tx.subscribe();

            handles.push(tokio::spawn(async move {
                tokio::select! {
                    result = http.run() => {
                        if let Err(e) = result {
                            error!(error = %e, "HTTP proxy error");
                        }
                    }
                    _ = shutdown_rx.recv() => {
                        debug!("HTTP proxy shutting down");
                    }
                }
            }));

            info!(bind = %self.config.proxy.http_bind, "HTTP proxy started");
        }

        // Monitor connection health
        let connection = self.connection.clone();
        let config = self.config.clone();
        let endpoint = self.endpoint.clone();
        let mut shutdown_rx = self.shutdown_tx.subscribe();

        handles.push(tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = tokio::time::sleep(Duration::from_secs(5)) => {
                        // Check connection health
                        let needs_reconnect = {
                            let conn = connection.read();
                            match &*conn {
                                Some(c) => c.close_reason().is_some(),
                                None => true,
                            }
                        };

                        if needs_reconnect {
                            warn!("Connection lost, attempting reconnect");
                            match reconnect(&endpoint, &config).await {
                                Ok(new_conn) => {
                                    let mut conn = connection.write();
                                    *conn = Some(new_conn);
                                    info!("Reconnected to server");
                                }
                                Err(e) => {
                                    error!(error = %e, "Reconnection failed");
                                }
                            }
                        }
                    }
                    _ = shutdown_rx.recv() => {
                        debug!("Connection monitor shutting down");
                        break;
                    }
                }
            }
        }));

        // Wait for all tasks
        for handle in handles {
            let _ = handle.await;
        }

        Ok(())
    }

    /// Shutdown the client
    pub async fn shutdown(&self) {
        let _ = self.shutdown_tx.send(());

        // Close connection
        if let Some(conn) = self.connection.write().take() {
            conn.close(quinn::VarInt::from_u32(0), b"client shutdown");
        }
    }
}

/// Shared handle for proxy servers to access the tunnel
pub struct TunnelClientHandle {
    connection: Arc<RwLock<Option<Connection>>>,
    config: Arc<Config>,
    endpoint: Endpoint,
}

impl TunnelClientHandle {
    /// Open a bidirectional stream
    pub async fn open_stream(&self) -> Result<(SendStream, RecvStream)> {
        let conn = self.get_connection().await?;
        let (send, recv) = conn.open_bi().await.context("Failed to open stream")?;
        Ok((send, recv))
    }

    /// Send a datagram
    pub async fn send_datagram(&self, data: Bytes) -> Result<()> {
        let conn = self.get_connection().await?;
        conn.send_datagram(data)
            .context("Failed to send datagram")?;
        Ok(())
    }

    /// Receive a datagram
    pub async fn recv_datagram(&self) -> Result<Bytes> {
        let conn = self.get_connection().await?;
        let data = conn
            .read_datagram()
            .await
            .context("Failed to receive datagram")?;
        Ok(data)
    }

    /// Get the current connection
    async fn get_connection(&self) -> Result<Connection> {
        // Check existing connection
        {
            let conn = self.connection.read();
            if let Some(ref c) = *conn {
                if !c.close_reason().is_some() {
                    return Ok(c.clone());
                }
            }
        }

        // Need to reconnect
        let new_conn = reconnect(&self.endpoint, &self.config).await?;

        {
            let mut conn = self.connection.write();
            *conn = Some(new_conn.clone());
        }

        Ok(new_conn)
    }
}

/// Create QUIC client endpoint
fn create_client_endpoint(config: &Config) -> Result<Endpoint> {
    // Configure TLS
    let mut root_store = rustls::RootCertStore::empty();

    if config.server.insecure {
        warn!("TLS certificate verification disabled (insecure mode)");
    } else {
        // Add webpki roots
        root_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    }

    let mut tls_config = if config.server.insecure {
        rustls::ClientConfig::builder()
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(InsecureServerVerifier))
            .with_no_client_auth()
    } else {
        rustls::ClientConfig::builder()
            .with_root_certificates(root_store)
            .with_no_client_auth()
    };

    tls_config.alpn_protocols = vec![b"mytunnel".to_vec()];

    // Configure QUIC
    let mut transport = quinn::TransportConfig::default();
    transport.max_idle_timeout(Some(
        Duration::from_secs(config.quic.idle_timeout_secs)
            .try_into()
            .unwrap(),
    ));
    transport.keep_alive_interval(Some(Duration::from_secs(10)));

    let mut client_config = quinn::ClientConfig::new(Arc::new(
        quinn::crypto::rustls::QuicClientConfig::try_from(tls_config)?,
    ));
    client_config.transport_config(Arc::new(transport));

    // Create endpoint
    let mut endpoint = Endpoint::client("0.0.0.0:0".parse()?)?;
    endpoint.set_default_client_config(client_config);

    Ok(endpoint)
}

/// Resolve server address
async fn resolve_address(address: &str) -> Result<SocketAddr> {
    // Try parsing as socket address first
    if let Ok(addr) = address.parse::<SocketAddr>() {
        return Ok(addr);
    }

    // DNS resolution
    let addrs: Vec<SocketAddr> = tokio::net::lookup_host(address)
        .await
        .with_context(|| format!("Failed to resolve {}", address))?
        .collect();

    addrs
        .into_iter()
        .next()
        .ok_or_else(|| anyhow::anyhow!("No addresses found for {}", address))
}

/// Reconnect to server
async fn reconnect(endpoint: &Endpoint, config: &Config) -> Result<Connection> {
    let server_addr = resolve_address(&config.server.address).await?;
    let server_name = config.server.get_server_name().to_string();

    let connection = endpoint
        .connect(server_addr, &server_name)?
        .await
        .context("Failed to reconnect")?;

    Ok(connection)
}

/// Insecure TLS verifier for development
#[derive(Debug)]
struct InsecureServerVerifier;

impl rustls::client::danger::ServerCertVerifier for InsecureServerVerifier {
    fn verify_server_cert(
        &self,
        _end_entity: &rustls::pki_types::CertificateDer<'_>,
        _intermediates: &[rustls::pki_types::CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &rustls::pki_types::CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &rustls::pki_types::CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        vec![
            rustls::SignatureScheme::RSA_PKCS1_SHA256,
            rustls::SignatureScheme::RSA_PKCS1_SHA384,
            rustls::SignatureScheme::RSA_PKCS1_SHA512,
            rustls::SignatureScheme::ECDSA_NISTP256_SHA256,
            rustls::SignatureScheme::ECDSA_NISTP384_SHA384,
            rustls::SignatureScheme::ECDSA_NISTP521_SHA512,
            rustls::SignatureScheme::RSA_PSS_SHA256,
            rustls::SignatureScheme::RSA_PSS_SHA384,
            rustls::SignatureScheme::RSA_PSS_SHA512,
            rustls::SignatureScheme::ED25519,
        ]
    }
}

