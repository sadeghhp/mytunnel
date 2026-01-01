//! SOCKS5 proxy server implementation
//!
//! Implements RFC 1928 SOCKS5 protocol with CONNECT and UDP ASSOCIATE support.

use anyhow::{Context, Result};
use bytes::BytesMut;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tracing::{debug, error, info, warn};

use crate::protocol::socks5::*;
use crate::tunnel::datagram::UdpAssociation;
use crate::tunnel::stream::{establish_tcp_tunnel, proxy_bidirectional};
use crate::tunnel::TunnelClientHandle;

/// SOCKS5 proxy server
pub struct Socks5Proxy {
    tunnel: Arc<TunnelClientHandle>,
    bind_addr: SocketAddr,
}

impl Socks5Proxy {
    /// Create a new SOCKS5 proxy
    pub fn new(tunnel: Arc<TunnelClientHandle>, bind_addr: SocketAddr) -> Self {
        Self { tunnel, bind_addr }
    }

    /// Run the SOCKS5 proxy server
    pub async fn run(&self) -> Result<()> {
        let listener = TcpListener::bind(self.bind_addr)
            .await
            .with_context(|| format!("Failed to bind SOCKS5 proxy to {}", self.bind_addr))?;

        info!(bind = %self.bind_addr, "SOCKS5 proxy listening");

        loop {
            match listener.accept().await {
                Ok((stream, client_addr)) => {
                    debug!(client = %client_addr, "New SOCKS5 connection");
                    let tunnel = self.tunnel.clone();

                    tokio::spawn(async move {
                        if let Err(e) = handle_socks5_client(stream, tunnel, client_addr).await {
                            debug!(error = %e, client = %client_addr, "SOCKS5 client error");
                        }
                    });
                }
                Err(e) => {
                    error!(error = %e, "Failed to accept connection");
                }
            }
        }
    }
}

/// Handle a single SOCKS5 client connection
async fn handle_socks5_client(
    mut stream: TcpStream,
    tunnel: Arc<TunnelClientHandle>,
    client_addr: SocketAddr,
) -> Result<()> {
    // Read version and auth methods
    let mut header = [0u8; 2];
    stream.read_exact(&mut header).await?;

    if header[0] != VERSION {
        return Err(anyhow::anyhow!("Invalid SOCKS version: {}", header[0]));
    }

    let nmethods = header[1] as usize;
    let mut methods = vec![0u8; nmethods];
    stream.read_exact(&mut methods).await?;

    // We only support no authentication
    let method = if methods.contains(&AUTH_NONE) {
        AUTH_NONE
    } else {
        AUTH_NO_ACCEPTABLE
    };

    // Send method selection
    stream.write_all(&[VERSION, method]).await?;

    if method == AUTH_NO_ACCEPTABLE {
        return Err(anyhow::anyhow!("No acceptable auth method"));
    }

    // Read request
    let mut request_header = [0u8; 4];
    stream.read_exact(&mut request_header).await?;

    if request_header[0] != VERSION {
        return Err(anyhow::anyhow!("Invalid request version"));
    }

    let cmd = request_header[1];
    // request_header[2] is reserved

    // Read address
    let mut addr_data = BytesMut::new();

    // Read address type and address
    let atyp = request_header[3];
    addr_data.extend_from_slice(&[atyp]);

    match atyp {
        ATYP_IPV4 => {
            let mut buf = [0u8; 6]; // 4 bytes IP + 2 bytes port
            stream.read_exact(&mut buf).await?;
            addr_data.extend_from_slice(&buf);
        }
        ATYP_DOMAIN => {
            let mut len_buf = [0u8; 1];
            stream.read_exact(&mut len_buf).await?;
            let len = len_buf[0] as usize;
            addr_data.extend_from_slice(&len_buf);

            let mut domain = vec![0u8; len + 2]; // domain + port
            stream.read_exact(&mut domain).await?;
            addr_data.extend_from_slice(&domain);
        }
        ATYP_IPV6 => {
            let mut buf = [0u8; 18]; // 16 bytes IP + 2 bytes port
            stream.read_exact(&mut buf).await?;
            addr_data.extend_from_slice(&buf);
        }
        _ => {
            // Send error reply
            let reply = encode_reply(REP_ATYP_NOT_SUPPORTED, zero_bind_addr_v4());
            stream.write_all(&reply).await?;
            return Err(anyhow::anyhow!("Unsupported address type: {}", atyp));
        }
    }

    let (host, port) = parse_address(&mut addr_data)?;

    debug!(cmd = %cmd, host = %host, port = %port, "SOCKS5 request");

    match cmd {
        CMD_CONNECT => {
            handle_connect(stream, tunnel, &host, port).await?;
        }
        CMD_UDP_ASSOCIATE => {
            handle_udp_associate(stream, tunnel, client_addr).await?;
        }
        CMD_BIND => {
            // BIND not supported
            let reply = encode_reply(REP_CMD_NOT_SUPPORTED, zero_bind_addr_v4());
            stream.write_all(&reply).await?;
            return Err(anyhow::anyhow!("BIND command not supported"));
        }
        _ => {
            let reply = encode_reply(REP_CMD_NOT_SUPPORTED, zero_bind_addr_v4());
            stream.write_all(&reply).await?;
            return Err(anyhow::anyhow!("Unknown command: {}", cmd));
        }
    }

    Ok(())
}

/// Handle CONNECT command
async fn handle_connect(
    mut stream: TcpStream,
    tunnel: Arc<TunnelClientHandle>,
    host: &str,
    port: u16,
) -> Result<()> {
    // Open QUIC stream
    let (quic_send, quic_recv) = match tunnel.open_stream().await {
        Ok(s) => s,
        Err(e) => {
            warn!(error = %e, "Failed to open tunnel stream");
            let reply = encode_reply(REP_GENERAL_FAILURE, zero_bind_addr_v4());
            stream.write_all(&reply).await?;
            return Err(e);
        }
    };

    // Establish TCP tunnel
    let (quic_send, quic_recv) = match establish_tcp_tunnel(quic_send, quic_recv, host, port).await
    {
        Ok(s) => s,
        Err(e) => {
            warn!(error = %e, host = %host, port = %port, "Failed to establish tunnel");
            let reply = encode_reply(REP_HOST_UNREACHABLE, zero_bind_addr_v4());
            stream.write_all(&reply).await?;
            return Err(e);
        }
    };

    // Send success reply
    let reply = encode_reply(REP_SUCCESS, zero_bind_addr_v4());
    stream.write_all(&reply).await?;

    debug!(host = %host, port = %port, "SOCKS5 CONNECT established");

    // Split the TCP stream and proxy data
    let (local_read, local_write) = stream.into_split();

    let (tx, rx) = proxy_bidirectional(local_read, local_write, quic_send, quic_recv).await?;

    debug!(tx_bytes = %tx, rx_bytes = %rx, "SOCKS5 CONNECT completed");

    Ok(())
}

/// Handle UDP ASSOCIATE command
async fn handle_udp_associate(
    mut stream: TcpStream,
    tunnel: Arc<TunnelClientHandle>,
    _client_addr: SocketAddr,
) -> Result<()> {
    // Create UDP association
    // Bind to ephemeral port on localhost
    let bind_addr: SocketAddr = "127.0.0.1:0".parse().unwrap();

    let association = match UdpAssociation::new(tunnel, bind_addr).await {
        Ok(a) => a,
        Err(e) => {
            warn!(error = %e, "Failed to create UDP association");
            let reply = encode_reply(REP_GENERAL_FAILURE, zero_bind_addr_v4());
            stream.write_all(&reply).await?;
            return Err(e);
        }
    };

    let local_addr = association.local_addr()?;

    // Send success reply with the UDP relay address
    let reply = encode_reply(REP_SUCCESS, local_addr);
    stream.write_all(&reply).await?;

    info!(udp_addr = %local_addr, "SOCKS5 UDP ASSOCIATE established");

    // Run the UDP association until the TCP connection closes
    tokio::select! {
        result = association.run() => {
            if let Err(e) = result {
                debug!(error = %e, "UDP association error");
            }
        }
        _ = wait_for_tcp_close(&mut stream) => {
            debug!("SOCKS5 TCP connection closed, ending UDP association");
        }
    }

    Ok(())
}

/// Wait for TCP connection to close (used for UDP ASSOCIATE lifecycle)
async fn wait_for_tcp_close(stream: &mut TcpStream) {
    let mut buf = [0u8; 1];
    // When the client closes the TCP connection, this will return
    let _ = stream.read(&mut buf).await;
}

