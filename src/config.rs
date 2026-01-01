//! Configuration management
//!
//! Handles loading and validating server configuration from TOML files.

use anyhow::{Context, Result};
use serde::Deserialize;
use std::net::SocketAddr;
use std::path::Path;

/// Root configuration structure
#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub server: ServerConfig,
    pub quic: QuicConfig,
    pub tls: TlsConfig,
    pub pool: PoolConfig,
    pub metrics: MetricsConfig,
    pub logging: LoggingConfig,
    #[serde(default)]
    pub limits: LimitsConfig,
}

/// Server configuration
#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    /// Address to bind the QUIC listener
    pub bind_addr: SocketAddr,
    /// Number of worker threads (0 = auto)
    #[serde(default)]
    pub workers: usize,
}

impl ServerConfig {
    /// Get effective worker count (auto-detect if 0)
    pub fn effective_workers(&self) -> usize {
        if self.workers == 0 {
            num_cpus::get()
        } else {
            self.workers
        }
    }
}

/// QUIC protocol configuration
#[derive(Debug, Clone, Deserialize)]
pub struct QuicConfig {
    /// Maximum concurrent connections
    #[serde(default = "default_max_connections")]
    pub max_connections: u32,
    /// Maximum streams per connection
    #[serde(default = "default_max_streams")]
    pub max_streams_per_conn: u32,
    /// Connection idle timeout in seconds
    #[serde(default = "default_idle_timeout")]
    pub idle_timeout_secs: u64,
    /// Maximum UDP payload size
    #[serde(default = "default_max_udp_payload")]
    pub max_udp_payload: u16,
    /// Enable 0-RTT
    #[serde(default = "default_true")]
    pub enable_0rtt: bool,
    /// Congestion control algorithm
    #[serde(default = "default_congestion_control")]
    pub congestion_control: String,
}

/// TLS configuration
#[derive(Debug, Clone, Deserialize)]
pub struct TlsConfig {
    /// Path to certificate file
    pub cert_path: String,
    /// Path to private key file
    pub key_path: String,
    /// Auto-generate self-signed cert if missing
    #[serde(default)]
    pub auto_generate: bool,
}

/// Memory pool configuration
#[derive(Debug, Clone, Deserialize)]
pub struct PoolConfig {
    /// Number of 4KB buffers
    #[serde(default = "default_buffer_count_4k")]
    pub buffer_count_4k: usize,
    /// Number of 16KB buffers
    #[serde(default = "default_buffer_count_16k")]
    pub buffer_count_16k: usize,
    /// Number of 64KB buffers
    #[serde(default = "default_buffer_count_64k")]
    pub buffer_count_64k: usize,
    /// Maximum connection slots
    #[serde(default = "default_connection_slots")]
    pub connection_slots: usize,
}

/// Metrics configuration
#[derive(Debug, Clone, Deserialize)]
pub struct MetricsConfig {
    /// Enable metrics endpoint
    #[serde(default)]
    pub enabled: bool,
    /// Metrics server bind address
    #[serde(default = "default_metrics_addr")]
    pub bind_addr: SocketAddr,
}

/// Logging configuration
#[derive(Debug, Clone, Deserialize)]
pub struct LoggingConfig {
    /// Log level
    #[serde(default = "default_log_level")]
    pub level: String,
    /// Output format: "json" or "pretty"
    #[serde(default = "default_log_format")]
    pub format: String,
}

/// Resource limits configuration
#[derive(Debug, Clone, Deserialize, Default)]
pub struct LimitsConfig {
    /// Max bandwidth per connection (bytes/sec, 0 = unlimited)
    #[serde(default)]
    pub max_bandwidth_per_conn: u64,
    /// Max new connections per second
    #[serde(default = "default_max_new_conn")]
    pub max_new_conn_per_sec: u32,
    /// Max memory usage in MB (0 = unlimited)
    #[serde(default)]
    pub max_memory_mb: usize,
}

// Default value functions
fn default_max_connections() -> u32 { 100_000 }
fn default_max_streams() -> u32 { 100 }
fn default_idle_timeout() -> u64 { 30 }
fn default_max_udp_payload() -> u16 { 1350 }
fn default_true() -> bool { true }
fn default_congestion_control() -> String { "bbr".to_string() }
fn default_buffer_count_4k() -> usize { 16384 }
fn default_buffer_count_16k() -> usize { 4096 }
fn default_buffer_count_64k() -> usize { 1024 }
fn default_connection_slots() -> usize { 100_000 }
fn default_metrics_addr() -> SocketAddr { "127.0.0.1:9090".parse().unwrap() }
fn default_log_level() -> String { "info".to_string() }
fn default_log_format() -> String { "json".to_string() }
fn default_max_new_conn() -> u32 { 10_000 }

impl Config {
    /// Load configuration from a TOML file
    pub fn load(path: &Path) -> Result<Self> {
        let contents = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read config file: {:?}", path))?;
        
        let config: Config = toml::from_str(&contents)
            .with_context(|| "Failed to parse config file")?;
        
        config.validate()?;
        Ok(config)
    }

    /// Validate configuration values
    fn validate(&self) -> Result<()> {
        if self.quic.max_connections == 0 {
            anyhow::bail!("max_connections must be > 0");
        }
        if self.quic.max_streams_per_conn == 0 {
            anyhow::bail!("max_streams_per_conn must be > 0");
        }
        if self.quic.idle_timeout_secs == 0 {
            anyhow::bail!("idle_timeout_secs must be > 0");
        }
        if self.pool.connection_slots == 0 {
            anyhow::bail!("connection_slots must be > 0");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_workers() {
        let config = ServerConfig {
            bind_addr: "0.0.0.0:443".parse().unwrap(),
            workers: 0,
        };
        assert!(config.effective_workers() > 0);
    }
}

