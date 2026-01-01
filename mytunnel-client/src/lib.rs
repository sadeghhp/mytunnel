//! MyTunnel Client Library
//!
//! A QUIC-based tunnel client with SOCKS5 and HTTP proxy support.

pub mod config;
pub mod protocol;
pub mod proxy;
pub mod tunnel;

pub use config::Config;
pub use tunnel::TunnelClient;

/// Client version
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

