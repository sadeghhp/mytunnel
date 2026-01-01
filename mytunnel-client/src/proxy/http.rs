//! HTTP CONNECT proxy server implementation
//!
//! Implements HTTP CONNECT tunneling for TCP proxying.

use anyhow::{Context, Result};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tracing::{debug, error, info, warn};

use crate::tunnel::stream::{establish_tcp_tunnel, proxy_bidirectional};
use crate::tunnel::TunnelClientHandle;

/// HTTP CONNECT proxy server
pub struct HttpProxy {
    tunnel: Arc<TunnelClientHandle>,
    bind_addr: SocketAddr,
}

impl HttpProxy {
    /// Create a new HTTP proxy
    pub fn new(tunnel: Arc<TunnelClientHandle>, bind_addr: SocketAddr) -> Self {
        Self { tunnel, bind_addr }
    }

    /// Run the HTTP proxy server
    pub async fn run(&self) -> Result<()> {
        let listener = TcpListener::bind(self.bind_addr)
            .await
            .with_context(|| format!("Failed to bind HTTP proxy to {}", self.bind_addr))?;

        info!(bind = %self.bind_addr, "HTTP proxy listening");

        loop {
            match listener.accept().await {
                Ok((stream, client_addr)) => {
                    debug!(client = %client_addr, "New HTTP connection");
                    let tunnel = self.tunnel.clone();

                    tokio::spawn(async move {
                        if let Err(e) = handle_http_client(stream, tunnel).await {
                            debug!(error = %e, client = %client_addr, "HTTP client error");
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

/// Handle a single HTTP client connection
async fn handle_http_client(stream: TcpStream, tunnel: Arc<TunnelClientHandle>) -> Result<()> {
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);

    // Read the request line
    let mut request_line = String::new();
    reader.read_line(&mut request_line).await?;

    let parts: Vec<&str> = request_line.trim().split_whitespace().collect();
    if parts.len() < 3 {
        send_error(&mut writer, 400, "Bad Request").await?;
        return Err(anyhow::anyhow!("Invalid request line"));
    }

    let method = parts[0];
    let target = parts[1];
    let _version = parts[2];

    // Only support CONNECT method
    if method != "CONNECT" {
        send_error(&mut writer, 405, "Method Not Allowed").await?;
        return Err(anyhow::anyhow!("Only CONNECT method supported, got: {}", method));
    }

    // Parse target (host:port)
    let (host, port) = parse_connect_target(target)?;

    // Read and discard headers until empty line
    loop {
        let mut line = String::new();
        reader.read_line(&mut line).await?;
        if line.trim().is_empty() {
            break;
        }
    }

    debug!(host = %host, port = %port, "HTTP CONNECT request");

    // Open QUIC stream
    let (quic_send, quic_recv) = match tunnel.open_stream().await {
        Ok(s) => s,
        Err(e) => {
            warn!(error = %e, "Failed to open tunnel stream");
            send_error(&mut writer, 502, "Bad Gateway").await?;
            return Err(e);
        }
    };

    // Establish TCP tunnel
    let (quic_send, quic_recv) = match establish_tcp_tunnel(quic_send, quic_recv, &host, port).await
    {
        Ok(s) => s,
        Err(e) => {
            warn!(error = %e, host = %host, port = %port, "Failed to establish tunnel");
            send_error(&mut writer, 502, "Bad Gateway").await?;
            return Err(e);
        }
    };

    // Send success response
    writer
        .write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")
        .await?;

    debug!(host = %host, port = %port, "HTTP CONNECT established");

    // Proxy data bidirectionally
    let (tx, rx) = proxy_bidirectional(reader, writer, quic_send, quic_recv).await?;

    debug!(tx_bytes = %tx, rx_bytes = %rx, "HTTP CONNECT completed");

    Ok(())
}

/// Parse CONNECT target (host:port)
fn parse_connect_target(target: &str) -> Result<(String, u16)> {
    // Handle IPv6 addresses like [::1]:443
    if target.starts_with('[') {
        // IPv6
        if let Some(bracket_end) = target.find(']') {
            let host = &target[1..bracket_end];
            let port_part = &target[bracket_end + 1..];
            if let Some(port_str) = port_part.strip_prefix(':') {
                let port: u16 = port_str.parse().context("Invalid port")?;
                return Ok((host.to_string(), port));
            }
        }
        return Err(anyhow::anyhow!("Invalid IPv6 target: {}", target));
    }

    // Regular host:port
    let parts: Vec<&str> = target.rsplitn(2, ':').collect();
    if parts.len() != 2 {
        return Err(anyhow::anyhow!("Invalid target format: {}", target));
    }

    let port: u16 = parts[0].parse().context("Invalid port")?;
    let host = parts[1].to_string();

    Ok((host, port))
}

/// Send HTTP error response
async fn send_error<W: AsyncWriteExt + Unpin>(
    writer: &mut W,
    status: u16,
    message: &str,
) -> Result<()> {
    let response = format!(
        "HTTP/1.1 {} {}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        status, message
    );
    writer.write_all(response.as_bytes()).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_connect_target() {
        let (host, port) = parse_connect_target("example.com:443").unwrap();
        assert_eq!(host, "example.com");
        assert_eq!(port, 443);

        let (host, port) = parse_connect_target("192.168.1.1:8080").unwrap();
        assert_eq!(host, "192.168.1.1");
        assert_eq!(port, 8080);

        let (host, port) = parse_connect_target("[::1]:443").unwrap();
        assert_eq!(host, "::1");
        assert_eq!(port, 443);
    }
}

