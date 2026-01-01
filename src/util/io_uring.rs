//! io_uring helpers for Linux
//!
//! This module provides io_uring integration for zero-copy I/O operations.
//! Only available on Linux with kernel 5.6+.

#![cfg(target_os = "linux")]

use std::os::unix::io::RawFd;

/// Check if io_uring is available on this system
pub fn is_available() -> bool {
    // Try to probe for io_uring support
    // In a real implementation, we would use the io_uring crate
    // For now, check kernel version
    if let Ok(uname) = nix::sys::utsname::uname() {
        let release = uname.release().to_string_lossy();
        if let Some(major_str) = release.split('.').next() {
            if let Ok(major) = major_str.parse::<u32>() {
                return major >= 5;
            }
        }
    }
    false
}

/// Placeholder for io_uring-based splice operation
/// In production, this would use tokio-uring or io-uring crate
pub async fn splice_async(
    _fd_in: RawFd,
    _fd_out: RawFd,
    _len: usize,
) -> std::io::Result<usize> {
    // This is a placeholder - real implementation would use io_uring
    // For MVP, we fall back to regular splice() in proxy/tcp.rs
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "io_uring splice not yet implemented, using sync splice",
    ))
}

/// Placeholder for io_uring-based sendmmsg
pub async fn sendmmsg_async(
    _fd: RawFd,
    _messages: &[&[u8]],
) -> std::io::Result<usize> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "io_uring sendmmsg not yet implemented",
    ))
}

