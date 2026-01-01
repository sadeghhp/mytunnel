//! Connection state

use serde::Serialize;
use std::net::SocketAddr;
use std::time::Instant;

/// Unique connection identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ConnectionId(pub u64);

impl ConnectionId {
    /// Create from raw u64
    pub fn from_raw(id: u64) -> Self {
        Self(id)
    }

    /// Get raw value
    pub fn as_u64(&self) -> u64 {
        self.0
    }
}

impl std::fmt::Display for ConnectionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:016x}", self.0)
    }
}

/// Connection lifecycle state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionPhase {
    /// Connection is being established
    Connecting,
    /// Connection is active and ready
    Active,
    /// Connection is draining (graceful close)
    Draining,
    /// Connection is closed
    Closed,
}

/// Per-connection state
#[derive(Debug)]
pub struct ConnectionState {
    /// Unique identifier
    pub id: ConnectionId,
    /// Client address
    pub client_addr: SocketAddr,
    /// Connection phase
    pub phase: ConnectionPhase,
    /// Connection start time
    pub connected_at: Instant,
    /// Last activity time
    pub last_active: Instant,
    /// Bytes received
    pub bytes_rx: u64,
    /// Bytes sent
    pub bytes_tx: u64,
    /// Active streams count
    pub active_streams: u32,
    /// Active UDP flows count
    pub active_udp_flows: u32,
}

impl ConnectionState {
    /// Create new connection state
    pub fn new(id: ConnectionId, client_addr: SocketAddr) -> Self {
        let now = Instant::now();
        Self {
            id,
            client_addr,
            phase: ConnectionPhase::Connecting,
            connected_at: now,
            last_active: now,
            bytes_rx: 0,
            bytes_tx: 0,
            active_streams: 0,
            active_udp_flows: 0,
        }
    }

    /// Mark connection as active
    pub fn set_active(&mut self) {
        self.phase = ConnectionPhase::Active;
        self.touch();
    }

    /// Mark connection as draining
    pub fn set_draining(&mut self) {
        self.phase = ConnectionPhase::Draining;
    }

    /// Mark connection as closed
    pub fn set_closed(&mut self) {
        self.phase = ConnectionPhase::Closed;
    }

    /// Update last activity timestamp
    pub fn touch(&mut self) {
        self.last_active = Instant::now();
    }

    /// Record received bytes
    pub fn record_rx(&mut self, bytes: u64) {
        self.bytes_rx = self.bytes_rx.saturating_add(bytes);
        self.touch();
    }

    /// Record sent bytes
    pub fn record_tx(&mut self, bytes: u64) {
        self.bytes_tx = self.bytes_tx.saturating_add(bytes);
        self.touch();
    }

    /// Get connection duration
    pub fn duration(&self) -> std::time::Duration {
        self.connected_at.elapsed()
    }

    /// Get idle duration
    pub fn idle_duration(&self) -> std::time::Duration {
        self.last_active.elapsed()
    }

    /// Check if connection is active
    pub fn is_active(&self) -> bool {
        self.phase == ConnectionPhase::Active
    }

    /// Increment stream count
    pub fn stream_opened(&mut self) {
        self.active_streams = self.active_streams.saturating_add(1);
    }

    /// Decrement stream count
    pub fn stream_closed(&mut self) {
        self.active_streams = self.active_streams.saturating_sub(1);
    }

    /// Increment UDP flow count
    pub fn udp_flow_opened(&mut self) {
        self.active_udp_flows = self.active_udp_flows.saturating_add(1);
    }

    /// Decrement UDP flow count
    pub fn udp_flow_closed(&mut self) {
        self.active_udp_flows = self.active_udp_flows.saturating_sub(1);
    }

    /// Convert to serializable info
    pub fn to_info(&self) -> ConnectionInfo {
        ConnectionInfo {
            id: format!("{}", self.id),
            client_addr: self.client_addr.to_string(),
            phase: format!("{:?}", self.phase),
            duration_secs: self.duration().as_secs_f64(),
            idle_secs: self.idle_duration().as_secs_f64(),
            bytes_rx: self.bytes_rx,
            bytes_tx: self.bytes_tx,
            active_streams: self.active_streams,
            active_udp_flows: self.active_udp_flows,
        }
    }
}

/// Serializable connection information for API responses
#[derive(Debug, Clone, Serialize)]
pub struct ConnectionInfo {
    /// Connection ID (hex string)
    pub id: String,
    /// Client IP:port
    pub client_addr: String,
    /// Connection phase
    pub phase: String,
    /// Duration in seconds
    pub duration_secs: f64,
    /// Idle time in seconds
    pub idle_secs: f64,
    /// Bytes received
    pub bytes_rx: u64,
    /// Bytes sent
    pub bytes_tx: u64,
    /// Active TCP streams
    pub active_streams: u32,
    /// Active UDP flows
    pub active_udp_flows: u32,
}

