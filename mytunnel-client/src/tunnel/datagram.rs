//! Datagram handling for UDP relay
//!
//! Handles QUIC datagrams for UDP packet relay.

use anyhow::Result;
use bytes::Bytes;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};
use parking_lot::Mutex;
use tokio::net::UdpSocket;
use tracing::{debug, warn};

use crate::protocol;
use crate::tunnel::connection::TunnelClientHandle;

/// UDP association for SOCKS5 UDP ASSOCIATE
pub struct UdpAssociation {
    /// Local UDP socket for client communication
    local_socket: Arc<UdpSocket>,
    /// Tunnel client handle
    tunnel: Arc<TunnelClientHandle>,
}

impl UdpAssociation {
    /// Create a new UDP association
    pub async fn new(
        tunnel: Arc<TunnelClientHandle>,
        bind_addr: SocketAddr,
    ) -> Result<Self> {
        let local_socket = UdpSocket::bind(bind_addr).await?;

        Ok(Self {
            local_socket: Arc::new(local_socket),
            tunnel,
        })
    }

    /// Get the local bound address
    pub fn local_addr(&self) -> Result<SocketAddr> {
        Ok(self.local_socket.local_addr()?)
    }

    /// Run the UDP association
    pub async fn run(self) -> Result<()> {
        let socket = self.local_socket.clone();
        let tunnel = self.tunnel.clone();
        
        // Track pending requests for matching responses
        let pending: Arc<Mutex<HashMap<(String, u16), (SocketAddr, Instant)>>> =
            Arc::new(Mutex::new(HashMap::new()));

        let pending_clone = pending.clone();
        let tunnel_clone = tunnel.clone();
        let socket_clone = socket.clone();

        // Task to receive from local clients and forward to tunnel
        let local_to_tunnel = async move {
            let mut buf = vec![0u8; 65536];
            
            loop {
                match socket.recv_from(&mut buf).await {
                    Ok((len, client_addr)) => {
                        // Parse SOCKS5 UDP header
                        if len < 10 {
                            continue;
                        }

                        // SOCKS5 UDP header: RSV(2) | FRAG(1) | ATYP(1) | DST.ADDR | DST.PORT | DATA
                        let frag = buf[2];
                        if frag != 0 {
                            // We don't support fragmentation
                            warn!("UDP fragmentation not supported");
                            continue;
                        }

                        let atyp = buf[3];
                        let (host, port, data_start) = match atyp {
                            0x01 => {
                                // IPv4
                                if len < 10 {
                                    continue;
                                }
                                let ip = std::net::Ipv4Addr::new(buf[4], buf[5], buf[6], buf[7]);
                                let port = u16::from_be_bytes([buf[8], buf[9]]);
                                (ip.to_string(), port, 10)
                            }
                            0x03 => {
                                // Domain
                                let domain_len = buf[4] as usize;
                                if len < 7 + domain_len {
                                    continue;
                                }
                                let domain = match std::str::from_utf8(&buf[5..5 + domain_len]) {
                                    Ok(d) => d.to_string(),
                                    Err(_) => continue,
                                };
                                let port_start = 5 + domain_len;
                                let port = u16::from_be_bytes([buf[port_start], buf[port_start + 1]]);
                                (domain, port, port_start + 2)
                            }
                            0x04 => {
                                // IPv6
                                if len < 22 {
                                    continue;
                                }
                                let mut octets = [0u8; 16];
                                octets.copy_from_slice(&buf[4..20]);
                                let ip = std::net::Ipv6Addr::from(octets);
                                let port = u16::from_be_bytes([buf[20], buf[21]]);
                                (ip.to_string(), port, 22)
                            }
                            _ => continue,
                        };

                        let payload = &buf[data_start..len];

                        // Store pending request info
                        {
                            let mut p = pending.lock();
                            p.insert((host.clone(), port), (client_addr, Instant::now()));
                            
                            // Cleanup old entries
                            p.retain(|_, (_, t)| t.elapsed() < Duration::from_secs(30));
                        }

                        // Encode and send through tunnel
                        match protocol::encode_udp_packet(&host, port, payload) {
                            Ok(packet) => {
                                if let Err(e) = tunnel.send_datagram(Bytes::from(packet)).await {
                                    debug!(error = %e, "Failed to send UDP datagram");
                                }
                            }
                            Err(e) => {
                                debug!(error = %e, "Failed to encode UDP packet");
                            }
                        }
                    }
                    Err(e) => {
                        debug!(error = %e, "UDP receive error");
                        break;
                    }
                }
            }
        };

        // Task to receive from tunnel and forward to local clients
        let tunnel_to_local = async move {
            loop {
                match tunnel_clone.recv_datagram().await {
                    Ok(data) => {
                        // Decode the response
                        match protocol::decode_udp_packet(data) {
                            Ok(packet) => {
                                // Find the client that sent this request
                                let client_addr = {
                                    let p = pending_clone.lock();
                                    p.get(&(packet.host.clone(), packet.port))
                                        .map(|(addr, _)| *addr)
                                };

                                if let Some(client_addr) = client_addr {
                                    // Build SOCKS5 UDP response
                                    let mut response = Vec::new();
                                    response.extend_from_slice(&[0, 0, 0]); // RSV, FRAG
                                    response.push(0x03); // Domain type
                                    response.push(packet.host.len() as u8);
                                    response.extend_from_slice(packet.host.as_bytes());
                                    response.extend_from_slice(&packet.port.to_be_bytes());
                                    response.extend_from_slice(&packet.payload);

                                    if let Err(e) = socket_clone.send_to(&response, client_addr).await {
                                        debug!(error = %e, "Failed to send UDP response to client");
                                    }
                                }
                            }
                            Err(e) => {
                                debug!(error = %e, "Failed to decode UDP response");
                            }
                        }
                    }
                    Err(e) => {
                        debug!(error = %e, "Failed to receive datagram from tunnel");
                        break;
                    }
                }
            }
        };

        tokio::select! {
            _ = local_to_tunnel => {}
            _ = tunnel_to_local => {}
        }

        Ok(())
    }
}

