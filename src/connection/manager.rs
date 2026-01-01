//! Connection manager
//!
//! Manages connection lifecycle and provides fast lookup.

use dashmap::DashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast;
use tracing::{debug, info, warn};

use super::state::{ConnectionId, ConnectionInfo, ConnectionState};
use crate::metrics::METRICS;
use crate::pool::{ConnectionSlab, SlabHandle};

/// Connection manager configuration
pub struct ConnectionManagerConfig {
    /// Maximum concurrent connections
    pub max_connections: usize,
    /// Idle timeout for connections
    pub idle_timeout: Duration,
}

/// Manages all active connections
pub struct ConnectionManager {
    /// Connection state slab
    connections: ConnectionSlab<ConnectionState>,
    /// Fast lookup by connection ID
    id_to_handle: DashMap<ConnectionId, SlabHandle>,
    /// ID generator
    next_id: AtomicU64,
    /// Configuration
    config: ConnectionManagerConfig,
    /// Shutdown signal sender
    shutdown_tx: broadcast::Sender<()>,
}

impl ConnectionManager {
    /// Create a new connection manager
    pub fn new(config: ConnectionManagerConfig) -> Arc<Self> {
        let (shutdown_tx, _) = broadcast::channel(1);
        
        Arc::new(Self {
            connections: ConnectionSlab::new(config.max_connections),
            id_to_handle: DashMap::with_capacity(config.max_connections),
            next_id: AtomicU64::new(1),
            config,
            shutdown_tx,
        })
    }

    /// Register a new connection
    pub fn register(&self, client_addr: SocketAddr) -> Option<ConnectionId> {
        // Generate unique ID
        let id = ConnectionId::from_raw(self.next_id.fetch_add(1, Ordering::Relaxed));
        
        // Create connection state
        let state = ConnectionState::new(id, client_addr);

        // Insert into slab
        let handle = self.connections.insert(state)?;

        // Add to lookup map
        self.id_to_handle.insert(id, handle);

        METRICS.connection_opened();
        info!(conn_id = %id, %client_addr, "User connected");

        Some(id)
    }

    /// Mark connection as active (handshake complete)
    pub fn activate(&self, id: ConnectionId) {
        if let Some(handle) = self.id_to_handle.get(&id) {
            if let Some(mut state) = self.connections.get_mut(*handle) {
                state.set_active();
                debug!(conn_id = %id, "Connection activated");
            }
        }
    }

    /// Unregister a connection
    pub fn unregister(&self, id: ConnectionId) {
        if let Some((_, handle)) = self.id_to_handle.remove(&id) {
            if let Some(state) = self.connections.remove(handle) {
                METRICS.connection_closed();
                info!(
                    conn_id = %id,
                    client_addr = %state.client_addr,
                    duration_secs = state.duration().as_secs_f64(),
                    bytes_rx = state.bytes_rx,
                    bytes_tx = state.bytes_tx,
                    "User disconnected"
                );
            }
        }
    }

    /// Get connection state for reading
    pub fn get(&self, id: ConnectionId) -> Option<impl std::ops::Deref<Target = ConnectionState> + '_> {
        let handle = self.id_to_handle.get(&id)?;
        self.connections.get(*handle)
    }

    /// Get connection state for modification
    pub fn get_mut(&self, id: ConnectionId) -> Option<impl std::ops::DerefMut<Target = ConnectionState> + '_> {
        let handle = self.id_to_handle.get(&id)?;
        self.connections.get_mut(*handle)
    }

    /// Update connection activity and record traffic
    pub fn record_traffic(&self, id: ConnectionId, rx: u64, tx: u64) {
        if let Some(handle) = self.id_to_handle.get(&id) {
            if let Some(mut state) = self.connections.get_mut(*handle) {
                if rx > 0 {
                    state.record_rx(rx);
                    METRICS.bytes_rx(rx);
                }
                if tx > 0 {
                    state.record_tx(tx);
                    METRICS.bytes_tx(tx);
                }
            }
        }
    }

    /// Get current connection count
    pub fn connection_count(&self) -> usize {
        self.connections.len()
    }

    /// List all active connections
    pub fn list_connections(&self) -> Vec<ConnectionInfo> {
        self.id_to_handle
            .iter()
            .filter_map(|entry| {
                self.connections.get(*entry.value()).map(|state| state.to_info())
            })
            .collect()
    }

    /// Check if at capacity
    pub fn is_full(&self) -> bool {
        self.connections.is_full()
    }

    /// Get shutdown receiver
    pub fn subscribe_shutdown(&self) -> broadcast::Receiver<()> {
        self.shutdown_tx.subscribe()
    }

    /// Signal shutdown to all connections
    pub fn signal_shutdown(&self) {
        info!("Signaling shutdown to all connections");
        let _ = self.shutdown_tx.send(());
    }

    /// Drain all connections (graceful shutdown)
    pub async fn drain(&self, timeout: Duration) {
        info!(
            connections = self.connection_count(),
            "Starting connection drain"
        );

        // Mark all connections as draining
        for entry in self.id_to_handle.iter() {
            if let Some(mut state) = self.connections.get_mut(*entry.value()) {
                state.set_draining();
            }
        }

        // Wait for connections to close or timeout
        let start = std::time::Instant::now();
        while self.connection_count() > 0 && start.elapsed() < timeout {
            tokio::time::sleep(Duration::from_millis(100)).await;
        }

        let remaining = self.connection_count();
        if remaining > 0 {
            warn!(remaining, "Force closing remaining connections after drain timeout");
        } else {
            info!("All connections drained successfully");
        }
    }

    /// Cleanup idle connections
    pub fn cleanup_idle(&self) -> usize {
        let mut cleaned = 0;
        let idle_timeout = self.config.idle_timeout;

        // Collect IDs to remove (can't remove while iterating)
        let to_remove: Vec<ConnectionId> = self
            .id_to_handle
            .iter()
            .filter_map(|entry| {
                if let Some(state) = self.connections.get(*entry.value()) {
                    if state.idle_duration() > idle_timeout {
                        return Some(*entry.key());
                    }
                }
                None
            })
            .collect();

        for id in to_remove {
            self.unregister(id);
            cleaned += 1;
        }

        if cleaned > 0 {
            debug!(cleaned, "Cleaned up idle connections");
        }

        cleaned
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_connection_lifecycle() {
        let config = ConnectionManagerConfig {
            max_connections: 100,
            idle_timeout: Duration::from_secs(30),
        };
        let manager = ConnectionManager::new(config);

        let addr: SocketAddr = "127.0.0.1:12345".parse().unwrap();
        let id = manager.register(addr).unwrap();

        assert_eq!(manager.connection_count(), 1);

        manager.activate(id);
        {
            let state = manager.get(id).unwrap();
            assert!(state.is_active());
        }

        manager.unregister(id);
        assert_eq!(manager.connection_count(), 0);
    }
}

