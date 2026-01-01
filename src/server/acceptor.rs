//! Connection acceptor and handler
//!
//! Handles individual QUIC connections after acceptance.

use anyhow::Result;
use bytes::Bytes;
use quinn::{Connection, Incoming, RecvStream, SendStream};
use std::sync::Arc;
use tracing::{debug, info, instrument, warn, Span};

use crate::config::Config;
use crate::connection::{ConnectionId, ConnectionManager};
use crate::metrics::METRICS;
use crate::pool::BufferPool;
use crate::proxy::{TcpProxy, UdpRelay};

/// Handles a single QUIC connection
pub struct ConnectionHandler {
    conn_manager: Arc<ConnectionManager>,
    buffer_pool: BufferPool,
    #[allow(dead_code)]
    config: Arc<Config>,
}

impl ConnectionHandler {
    /// Create a new connection handler
    pub fn new(
        conn_manager: Arc<ConnectionManager>,
        buffer_pool: BufferPool,
        config: Arc<Config>,
    ) -> Self {
        Self {
            conn_manager,
            buffer_pool,
            config,
        }
    }

    /// Handle an incoming connection
    #[instrument(skip(self, incoming), fields(client_addr))]
    pub async fn handle(self, incoming: Incoming) -> Result<()> {
        let client_addr = incoming.remote_address();
        Span::current().record("client_addr", client_addr.to_string());

        // Accept the connection
        let connection = match incoming.await {
            Ok(conn) => conn,
            Err(e) => {
                METRICS.connection_failed();
                return Err(e.into());
            }
        };

        // Register connection
        let conn_id = match self.conn_manager.register(client_addr) {
            Some(id) => id,
            None => {
                warn!("Failed to register connection: pool full");
                connection.close(quinn::VarInt::from_u32(1), b"server at capacity");
                return Ok(());
            }
        };

        info!(conn_id = %conn_id, "Connection established");
        self.conn_manager.activate(conn_id);

        // Get shutdown signal
        let mut shutdown_rx = self.conn_manager.subscribe_shutdown();

        // Handle connection until closed
        let result = self
            .handle_connection(conn_id, connection.clone(), &mut shutdown_rx)
            .await;

        // Cleanup
        self.conn_manager.unregister(conn_id);

        if let Err(e) = &result {
            debug!(conn_id = %conn_id, error = %e, "Connection closed with error");
        } else {
            debug!(conn_id = %conn_id, "Connection closed normally");
        }

        Ok(())
    }

    /// Main connection handling loop
    async fn handle_connection(
        &self,
        conn_id: ConnectionId,
        connection: Connection,
        shutdown_rx: &mut tokio::sync::broadcast::Receiver<()>,
    ) -> Result<()> {
        loop {
            tokio::select! {
                // Handle bidirectional streams (TCP proxy requests)
                stream = connection.accept_bi() => {
                    match stream {
                        Ok((send, recv)) => {
                            METRICS.stream_opened();
                            let handler = StreamHandler {
                                conn_id,
                                conn_manager: self.conn_manager.clone(),
                                buffer_pool: self.buffer_pool.clone(),
                            };
                            tokio::spawn(async move {
                                if let Err(e) = handler.handle_stream(send, recv).await {
                                    debug!(error = %e, "Stream error");
                                }
                                METRICS.stream_closed();
                            });
                        }
                        Err(quinn::ConnectionError::ApplicationClosed(_)) => {
                            debug!(conn_id = %conn_id, "Connection closed by peer");
                            break;
                        }
                        Err(e) => {
                            debug!(conn_id = %conn_id, error = %e, "Stream accept error");
                            break;
                        }
                    }
                }

                // Handle datagrams (UDP relay)
                datagram = connection.read_datagram() => {
                    match datagram {
                        Ok(data) => {
                            METRICS.datagram_rx();
                            let handler = DatagramHandler {
                                conn_id,
                                connection: connection.clone(),
                                buffer_pool: self.buffer_pool.clone(),
                            };
                            tokio::spawn(async move {
                                if let Err(e) = handler.handle_datagram(data).await {
                                    debug!(error = %e, "Datagram error");
                                }
                            });
                        }
                        Err(quinn::ConnectionError::ApplicationClosed(_)) => {
                            break;
                        }
                        Err(e) => {
                            debug!(conn_id = %conn_id, error = %e, "Datagram receive error");
                            // Continue - datagrams are unreliable
                        }
                    }
                }

                // Shutdown signal
                _ = shutdown_rx.recv() => {
                    info!(conn_id = %conn_id, "Shutdown signal received, closing connection");
                    connection.close(quinn::VarInt::from_u32(0), b"server shutdown");
                    break;
                }
            }
        }

        Ok(())
    }
}

/// Handles a single bidirectional stream (TCP tunnel request)
struct StreamHandler {
    conn_id: ConnectionId,
    #[allow(dead_code)]
    conn_manager: Arc<ConnectionManager>,
    buffer_pool: BufferPool,
}

impl StreamHandler {
    /// Handle a bidirectional stream
    async fn handle_stream(self, mut send: SendStream, mut recv: RecvStream) -> Result<()> {
        // Read request header (target address)
        // Format: [1 byte type][2 bytes port][N bytes host]
        let mut header = [0u8; 3];
        recv.read_exact(&mut header).await?;

        let request_type = header[0];
        let port = u16::from_be_bytes([header[1], header[2]]);

        // Read host length and host
        let mut host_len_buf = [0u8; 1];
        recv.read_exact(&mut host_len_buf).await?;
        let host_len = host_len_buf[0] as usize;

        let mut host_buf = vec![0u8; host_len];
        recv.read_exact(&mut host_buf).await?;
        let host = String::from_utf8(host_buf)?;

        debug!(
            conn_id = %self.conn_id,
            request_type,
            host = %host,
            port,
            "Stream request"
        );

        match request_type {
            // TCP connect request
            0x01 => {
                let target = format!("{}:{}", host, port);
                
                // Send acknowledgment
                send.write_all(&[0x00]).await?; // Success
                
                // Start TCP proxy
                let proxy = TcpProxy::new(self.buffer_pool.clone());
                proxy.proxy_stream(send, recv, &target).await?;
            }
            // Unknown request type
            _ => {
                warn!(request_type, "Unknown request type");
                send.write_all(&[0xFF]).await?; // Error
            }
        }

        Ok(())
    }
}

/// Handles datagrams (UDP relay)
struct DatagramHandler {
    conn_id: ConnectionId,
    connection: Connection,
    buffer_pool: BufferPool,
}

impl DatagramHandler {
    /// Handle a datagram
    async fn handle_datagram(self, data: Bytes) -> Result<()> {
        // Parse datagram header
        // Format: [2 bytes port][N bytes host][payload]
        if data.len() < 4 {
            return Ok(());
        }

        let port = u16::from_be_bytes([data[0], data[1]]);
        let host_len = data[2] as usize;

        if data.len() < 3 + host_len {
            return Ok(());
        }

        let host = std::str::from_utf8(&data[3..3 + host_len])?;
        let payload = &data[3 + host_len..];

        debug!(
            conn_id = %self.conn_id,
            host = %host,
            port,
            payload_len = payload.len(),
            "Datagram relay"
        );

        // Relay UDP packet
        let relay = UdpRelay::new(self.buffer_pool.clone());
        let target = format!("{}:{}", host, port);
        
        if let Ok(response) = relay.relay_packet(&target, payload).await {
            // Send response back through QUIC datagram
            let mut response_buf = Vec::with_capacity(3 + host_len + response.len());
            response_buf.extend_from_slice(&port.to_be_bytes());
            response_buf.push(host_len as u8);
            response_buf.extend_from_slice(host.as_bytes());
            response_buf.extend_from_slice(&response);
            
            let _ = self.connection.send_datagram(Bytes::from(response_buf));
            METRICS.datagram_tx();
        }

        Ok(())
    }
}

