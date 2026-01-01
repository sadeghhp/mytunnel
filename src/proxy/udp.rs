//! UDP relay with batched sending
//!
//! Uses sendmmsg() for efficient batch packet sending.

use anyhow::{Context, Result};
use dashmap::DashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::net::UdpSocket;

use crate::metrics::METRICS;
use crate::pool::BufferPool;

/// Maximum number of packets to batch
#[cfg(target_os = "linux")]
const MAX_BATCH_SIZE: usize = 64;

/// UDP socket pool entry TTL
const SOCKET_TTL: Duration = Duration::from_secs(60);

/// UDP relay for datagram forwarding
pub struct UdpRelay {
    #[allow(dead_code)]
    buffer_pool: BufferPool,
    /// Socket pool for reusing connections
    socket_pool: Arc<UdpSocketPool>,
}

impl UdpRelay {
    /// Create a new UDP relay
    pub fn new(buffer_pool: BufferPool) -> Self {
        Self {
            buffer_pool,
            socket_pool: Arc::new(UdpSocketPool::new()),
        }
    }

    /// Relay a single UDP packet and wait for response
    pub async fn relay_packet(&self, target: &str, data: &[u8]) -> Result<Vec<u8>> {
        // Resolve target address
        let target_addr: SocketAddr = tokio::net::lookup_host(target)
            .await?
            .next()
            .ok_or_else(|| anyhow::anyhow!("Failed to resolve {}", target))?;

        // Get or create socket
        let socket = self.socket_pool.get_or_create(target_addr).await?;

        // Send packet
        socket
            .send_to(data, target_addr)
            .await
            .context("Failed to send UDP packet")?;

        // Wait for response with timeout
        let mut response_buf = vec![0u8; 65536];
        let timeout = Duration::from_secs(5);

        match tokio::time::timeout(timeout, socket.recv_from(&mut response_buf)).await {
            Ok(Ok((n, _))) => {
                response_buf.truncate(n);
                Ok(response_buf)
            }
            Ok(Err(e)) => Err(e.into()),
            Err(_) => Err(anyhow::anyhow!("UDP response timeout")),
        }
    }

    /// Relay multiple packets in a batch (for efficiency)
    #[cfg(target_os = "linux")]
    pub async fn relay_batch(&self, packets: &[(SocketAddr, &[u8])]) -> Result<usize> {
        if packets.is_empty() {
            return Ok(0);
        }

        // For batch sending, we use sendmmsg
        // This requires a single destination, so group by target
        // For simplicity in MVP, we send individually but could optimize later

        let mut sent = 0;
        for (target, data) in packets.iter().take(MAX_BATCH_SIZE) {
            let socket = self.socket_pool.get_or_create(*target).await?;
            if socket.send_to(data, target).await.is_ok() {
                sent += 1;
                METRICS.datagram_tx();
            }
        }

        Ok(sent)
    }
}

/// Socket pool for UDP connections
struct UdpSocketPool {
    /// Map of target -> (socket, last_used)
    sockets: DashMap<SocketAddr, (Arc<UdpSocket>, Instant)>,
}

impl UdpSocketPool {
    fn new() -> Self {
        Self {
            sockets: DashMap::new(),
        }
    }

    /// Get or create a socket for the target
    async fn get_or_create(&self, target: SocketAddr) -> Result<Arc<UdpSocket>> {
        // Check existing socket
        if let Some(entry) = self.sockets.get(&target) {
            let (socket, last_used) = entry.value();
            if last_used.elapsed() < SOCKET_TTL {
                return Ok(socket.clone());
            }
        }

        // Create new socket
        let bind_addr: SocketAddr = if target.is_ipv4() {
            "0.0.0.0:0".parse().unwrap()
        } else {
            "[::]:0".parse().unwrap()
        };

        let socket = UdpSocket::bind(bind_addr)
            .await
            .context("Failed to bind UDP socket")?;

        let socket = Arc::new(socket);
        self.sockets.insert(target, (socket.clone(), Instant::now()));

        // Cleanup old sockets periodically
        self.cleanup_stale();

        Ok(socket)
    }

    /// Remove stale sockets
    fn cleanup_stale(&self) {
        self.sockets.retain(|_, (_, last_used)| {
            last_used.elapsed() < SOCKET_TTL * 2
        });
    }
}

/// Batched UDP sender using sendmmsg (Linux only)
#[cfg(target_os = "linux")]
#[allow(dead_code)]
pub struct BatchedUdpSender {
    socket: std::os::unix::io::RawFd,
}

#[cfg(target_os = "linux")]
#[allow(dead_code)]
impl BatchedUdpSender {
    /// Create from raw file descriptor
    pub fn from_raw_fd(fd: std::os::unix::io::RawFd) -> Self {
        Self { socket: fd }
    }

    /// Send multiple packets in a single syscall
    pub fn send_batch(&self, packets: &[(SocketAddr, &[u8])]) -> std::io::Result<usize> {
        use libc::{mmsghdr, msghdr, iovec, sendmmsg, sockaddr_storage, socklen_t};
        use std::mem::MaybeUninit;
        use std::ptr;

        if packets.is_empty() {
            return Ok(0);
        }

        let batch_size = packets.len().min(MAX_BATCH_SIZE);
        
        // Prepare message headers
        let mut msgs: Vec<MaybeUninit<mmsghdr>> = vec![MaybeUninit::uninit(); batch_size];
        let mut iovecs: Vec<iovec> = Vec::with_capacity(batch_size);
        let mut addrs: Vec<sockaddr_storage> = vec![unsafe { std::mem::zeroed() }; batch_size];

        for (i, (addr, data)) in packets.iter().enumerate().take(batch_size) {
            // Convert SocketAddr to sockaddr_storage
            let (sockaddr_ptr, sockaddr_len) = match addr {
                SocketAddr::V4(v4) => {
                    let sin = libc::sockaddr_in {
                        sin_family: libc::AF_INET as _,
                        sin_port: v4.port().to_be(),
                        sin_addr: libc::in_addr {
                            s_addr: u32::from_ne_bytes(v4.ip().octets()),
                        },
                        sin_zero: [0; 8],
                    };
                    unsafe {
                        ptr::copy_nonoverlapping(
                            &sin as *const _ as *const u8,
                            &mut addrs[i] as *mut _ as *mut u8,
                            std::mem::size_of::<libc::sockaddr_in>(),
                        );
                    }
                    (
                        &addrs[i] as *const _ as *const libc::sockaddr,
                        std::mem::size_of::<libc::sockaddr_in>() as socklen_t,
                    )
                }
                SocketAddr::V6(v6) => {
                    let sin6 = libc::sockaddr_in6 {
                        sin6_family: libc::AF_INET6 as _,
                        sin6_port: v6.port().to_be(),
                        sin6_flowinfo: v6.flowinfo(),
                        sin6_addr: libc::in6_addr {
                            s6_addr: v6.ip().octets(),
                        },
                        sin6_scope_id: v6.scope_id(),
                    };
                    unsafe {
                        ptr::copy_nonoverlapping(
                            &sin6 as *const _ as *const u8,
                            &mut addrs[i] as *mut _ as *mut u8,
                            std::mem::size_of::<libc::sockaddr_in6>(),
                        );
                    }
                    (
                        &addrs[i] as *const _ as *const libc::sockaddr,
                        std::mem::size_of::<libc::sockaddr_in6>() as socklen_t,
                    )
                }
            };

            // Setup iovec
            iovecs.push(iovec {
                iov_base: data.as_ptr() as *mut _,
                iov_len: data.len(),
            });

            // Setup msghdr
            let hdr = msghdr {
                msg_name: sockaddr_ptr as *mut _,
                msg_namelen: sockaddr_len,
                msg_iov: &mut iovecs[i],
                msg_iovlen: 1,
                msg_control: ptr::null_mut(),
                msg_controllen: 0,
                msg_flags: 0,
            };

            msgs[i].write(mmsghdr {
                msg_hdr: hdr,
                msg_len: 0,
            });
        }

        // Send all packets in single syscall
        let msgs_ptr = msgs.as_mut_ptr() as *mut mmsghdr;
        let result = unsafe { sendmmsg(self.socket, msgs_ptr, batch_size as _, 0) };

        if result < 0 {
            Err(std::io::Error::last_os_error())
        } else {
            Ok(result as usize)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_udp_relay_creation() {
        let pool = BufferPool::new(10, 5, 2);
        let _relay = UdpRelay::new(pool);
    }

    #[tokio::test]
    async fn test_socket_pool() {
        let pool = UdpSocketPool::new();
        let addr: SocketAddr = "8.8.8.8:53".parse().unwrap();
        
        let socket1 = pool.get_or_create(addr).await.unwrap();
        let socket2 = pool.get_or_create(addr).await.unwrap();
        
        // Should return same socket
        assert!(Arc::ptr_eq(&socket1, &socket2));
    }
}

