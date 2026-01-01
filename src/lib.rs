//! MyTunnel Server - High-performance QUIC-based tunnel
//!
//! This library provides the core components for a high-performance
//! tunnel server using QUIC transport with zero-copy forwarding.

pub mod config;
pub mod connection;
pub mod metrics;
pub mod pool;
pub mod proxy;
pub mod router;
pub mod server;
pub mod util;

pub use config::Config;
pub use server::Server;

/// Server version for display
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

