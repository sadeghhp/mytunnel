//! Stream management for TCP tunneling
//!
//! Handles bidirectional QUIC streams for TCP proxy requests.

use anyhow::{Context, Result};
use quinn::{RecvStream, SendStream};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tracing::debug;

use crate::protocol;

/// Establish a TCP tunnel through a QUIC stream
pub async fn establish_tcp_tunnel(
    mut send: SendStream,
    mut recv: RecvStream,
    host: &str,
    port: u16,
) -> Result<(SendStream, RecvStream)> {
    // Send TCP connect request
    let request = protocol::encode_tcp_request(host, port)?;
    send.write_all(&request)
        .await
        .context("Failed to send tunnel request")?;

    // Read response
    let mut response = [0u8; 1];
    recv.read_exact(&mut response)
        .await
        .context("Failed to read tunnel response")?;

    protocol::decode_tcp_response(&response)?;

    debug!(host = %host, port = %port, "TCP tunnel established");

    Ok((send, recv))
}

/// Proxy data between a local TCP stream and QUIC stream
pub async fn proxy_bidirectional<R, W>(
    mut local_read: R,
    mut local_write: W,
    mut quic_send: SendStream,
    mut quic_recv: RecvStream,
) -> Result<(u64, u64)>
where
    R: tokio::io::AsyncRead + Unpin,
    W: tokio::io::AsyncWrite + Unpin,
{
    let local_to_remote = async {
        let mut buf = vec![0u8; 16384];
        let mut total: u64 = 0;

        loop {
            match local_read.read(&mut buf).await {
                Ok(0) => break, // EOF
                Ok(n) => {
                    if quic_send.write_all(&buf[..n]).await.is_err() {
                        break;
                    }
                    total += n as u64;
                }
                Err(_) => break,
            }
        }
        let _ = quic_send.finish();
        total
    };

    let remote_to_local = async {
        let mut buf = vec![0u8; 16384];
        let mut total: u64 = 0;

        loop {
            match quic_recv.read(&mut buf).await {
                Ok(Some(n)) if n > 0 => {
                    if local_write.write_all(&buf[..n]).await.is_err() {
                        break;
                    }
                    total += n as u64;
                }
                Ok(_) => break, // EOF or zero bytes
                Err(_) => break,
            }
        }
        let _ = local_write.shutdown().await;
        total
    };

    let (tx, rx) = tokio::join!(local_to_remote, remote_to_local);
    Ok((tx, rx))
}

