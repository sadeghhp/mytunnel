//! Socket utilities and tuning

use anyhow::Result;
use socket2::{Domain, Protocol, Socket, Type};
use std::net::SocketAddr;

/// Socket buffer sizes for high performance
pub const RECV_BUFFER_SIZE: usize = 8 * 1024 * 1024; // 8MB
pub const SEND_BUFFER_SIZE: usize = 8 * 1024 * 1024; // 8MB

/// Create an optimized UDP socket for QUIC
pub fn create_udp_socket(addr: SocketAddr, _reuse_port: bool) -> Result<std::net::UdpSocket> {
    let domain = if addr.is_ipv4() {
        Domain::IPV4
    } else {
        Domain::IPV6
    };

    let socket = Socket::new(domain, Type::DGRAM, Some(Protocol::UDP))?;

    // Enable address reuse
    socket.set_reuse_address(true)?;

    // Enable port reuse for multi-core scaling (Unix only)
    #[cfg(all(unix, not(target_os = "macos")))]
    if reuse_port {
        use std::os::unix::io::AsRawFd;
        unsafe {
            let optval: libc::c_int = 1;
            libc::setsockopt(
                socket.as_raw_fd(),
                libc::SOL_SOCKET,
                libc::SO_REUSEPORT,
                &optval as *const _ as *const libc::c_void,
                std::mem::size_of::<libc::c_int>() as libc::socklen_t,
            );
        }
    }

    // Set large buffer sizes for high throughput
    socket.set_recv_buffer_size(RECV_BUFFER_SIZE)?;
    socket.set_send_buffer_size(SEND_BUFFER_SIZE)?;

    // Non-blocking mode
    socket.set_nonblocking(true)?;

    // Bind to address
    socket.bind(&addr.into())?;

    Ok(socket.into())
}

/// Create an optimized TCP socket for proxying
pub fn create_tcp_socket(addr: SocketAddr) -> Result<Socket> {
    let domain = if addr.is_ipv4() {
        Domain::IPV4
    } else {
        Domain::IPV6
    };

    let socket = Socket::new(domain, Type::STREAM, Some(Protocol::TCP))?;

    // Enable address reuse
    socket.set_reuse_address(true)?;

    // Set buffer sizes
    socket.set_recv_buffer_size(RECV_BUFFER_SIZE)?;
    socket.set_send_buffer_size(SEND_BUFFER_SIZE)?;

    // TCP optimizations
    socket.set_nodelay(true)?; // Disable Nagle's algorithm
    socket.set_nonblocking(true)?;

    // TCP keepalive for connection health
    let keepalive = socket2::TcpKeepalive::new()
        .with_time(std::time::Duration::from_secs(60))
        .with_interval(std::time::Duration::from_secs(10));
    socket.set_tcp_keepalive(&keepalive)?;

    Ok(socket)
}

/// Apply socket optimizations for an existing socket
#[cfg(target_os = "linux")]
pub fn optimize_socket_linux(fd: std::os::unix::io::RawFd) -> Result<()> {
    use nix::sys::socket::{setsockopt, sockopt};

    // Enable busy polling for lower latency (requires root)
    let _ = setsockopt(fd, sockopt::Busy, &50);

    // Set priority for QoS
    let _ = setsockopt(fd, sockopt::Priority, &6);

    Ok(())
}

#[cfg(not(target_os = "linux"))]
pub fn optimize_socket_linux(_fd: std::os::unix::io::RawFd) -> Result<()> {
    Ok(())
}

