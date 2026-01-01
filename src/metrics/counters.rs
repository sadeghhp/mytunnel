//! Atomic counters for hot-path metrics
//!
//! Lock-free counters that can be safely updated from any thread.

use std::sync::atomic::{AtomicU64, Ordering};

/// Global metrics instance
pub static METRICS: Metrics = Metrics::new();

/// Atomic metrics counters
pub struct Metrics {
    // Connection metrics
    pub connections_total: AtomicU64,
    pub connections_active: AtomicU64,
    pub connections_failed: AtomicU64,

    // Traffic metrics
    pub bytes_received: AtomicU64,
    pub bytes_sent: AtomicU64,
    pub packets_received: AtomicU64,
    pub packets_sent: AtomicU64,

    // Stream metrics
    pub streams_opened: AtomicU64,
    pub streams_closed: AtomicU64,

    // UDP relay metrics
    pub datagrams_received: AtomicU64,
    pub datagrams_sent: AtomicU64,

    // Error metrics
    pub errors_total: AtomicU64,
    pub timeouts_total: AtomicU64,

    // Pool metrics
    pub buffer_pool_acquires: AtomicU64,
    pub buffer_pool_releases: AtomicU64,
    pub buffer_pool_misses: AtomicU64,
}

impl Metrics {
    pub const fn new() -> Self {
        Self {
            connections_total: AtomicU64::new(0),
            connections_active: AtomicU64::new(0),
            connections_failed: AtomicU64::new(0),
            bytes_received: AtomicU64::new(0),
            bytes_sent: AtomicU64::new(0),
            packets_received: AtomicU64::new(0),
            packets_sent: AtomicU64::new(0),
            streams_opened: AtomicU64::new(0),
            streams_closed: AtomicU64::new(0),
            datagrams_received: AtomicU64::new(0),
            datagrams_sent: AtomicU64::new(0),
            errors_total: AtomicU64::new(0),
            timeouts_total: AtomicU64::new(0),
            buffer_pool_acquires: AtomicU64::new(0),
            buffer_pool_releases: AtomicU64::new(0),
            buffer_pool_misses: AtomicU64::new(0),
        }
    }

    // Connection tracking
    #[inline]
    pub fn connection_opened(&self) {
        self.connections_total.fetch_add(1, Ordering::Relaxed);
        self.connections_active.fetch_add(1, Ordering::Relaxed);
    }

    #[inline]
    pub fn connection_closed(&self) {
        self.connections_active.fetch_sub(1, Ordering::Relaxed);
    }

    #[inline]
    pub fn connection_failed(&self) {
        self.connections_failed.fetch_add(1, Ordering::Relaxed);
    }

    // Traffic tracking
    #[inline]
    pub fn bytes_rx(&self, count: u64) {
        self.bytes_received.fetch_add(count, Ordering::Relaxed);
        self.packets_received.fetch_add(1, Ordering::Relaxed);
    }

    #[inline]
    pub fn bytes_tx(&self, count: u64) {
        self.bytes_sent.fetch_add(count, Ordering::Relaxed);
        self.packets_sent.fetch_add(1, Ordering::Relaxed);
    }

    // Stream tracking
    #[inline]
    pub fn stream_opened(&self) {
        self.streams_opened.fetch_add(1, Ordering::Relaxed);
    }

    #[inline]
    pub fn stream_closed(&self) {
        self.streams_closed.fetch_add(1, Ordering::Relaxed);
    }

    // Datagram tracking
    #[inline]
    pub fn datagram_rx(&self) {
        self.datagrams_received.fetch_add(1, Ordering::Relaxed);
    }

    #[inline]
    pub fn datagram_tx(&self) {
        self.datagrams_sent.fetch_add(1, Ordering::Relaxed);
    }

    // Error tracking
    #[inline]
    pub fn error(&self) {
        self.errors_total.fetch_add(1, Ordering::Relaxed);
    }

    #[inline]
    pub fn timeout(&self) {
        self.timeouts_total.fetch_add(1, Ordering::Relaxed);
    }

    // Buffer pool tracking
    #[inline]
    pub fn buffer_acquired(&self) {
        self.buffer_pool_acquires.fetch_add(1, Ordering::Relaxed);
    }

    #[inline]
    pub fn buffer_released(&self) {
        self.buffer_pool_releases.fetch_add(1, Ordering::Relaxed);
    }

    #[inline]
    pub fn buffer_miss(&self) {
        self.buffer_pool_misses.fetch_add(1, Ordering::Relaxed);
    }

    /// Get snapshot of all metrics
    pub fn snapshot(&self) -> MetricsSnapshot {
        MetricsSnapshot {
            connections_total: self.connections_total.load(Ordering::Relaxed),
            connections_active: self.connections_active.load(Ordering::Relaxed),
            connections_failed: self.connections_failed.load(Ordering::Relaxed),
            bytes_received: self.bytes_received.load(Ordering::Relaxed),
            bytes_sent: self.bytes_sent.load(Ordering::Relaxed),
            packets_received: self.packets_received.load(Ordering::Relaxed),
            packets_sent: self.packets_sent.load(Ordering::Relaxed),
            streams_opened: self.streams_opened.load(Ordering::Relaxed),
            streams_closed: self.streams_closed.load(Ordering::Relaxed),
            datagrams_received: self.datagrams_received.load(Ordering::Relaxed),
            datagrams_sent: self.datagrams_sent.load(Ordering::Relaxed),
            errors_total: self.errors_total.load(Ordering::Relaxed),
            timeouts_total: self.timeouts_total.load(Ordering::Relaxed),
        }
    }
}

/// Snapshot of metrics for reporting
#[derive(Debug, Clone)]
pub struct MetricsSnapshot {
    pub connections_total: u64,
    pub connections_active: u64,
    pub connections_failed: u64,
    pub bytes_received: u64,
    pub bytes_sent: u64,
    pub packets_received: u64,
    pub packets_sent: u64,
    pub streams_opened: u64,
    pub streams_closed: u64,
    pub datagrams_received: u64,
    pub datagrams_sent: u64,
    pub errors_total: u64,
    pub timeouts_total: u64,
}

