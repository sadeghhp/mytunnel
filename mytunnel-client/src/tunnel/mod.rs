//! Tunnel management
//!
//! Handles QUIC connection to the server and manages streams/datagrams.

pub mod connection;
pub mod datagram;
pub mod stream;

pub use connection::{TunnelClient, TunnelClientHandle};

