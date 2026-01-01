//! Configuration management
//!
//! Handles loading and validating client configuration from TOML files.

use anyhow::{Context, Result};
use serde::Deserialize;
use std::net::SocketAddr;
use std::path::Path;

/// Root configuration structure
#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub server: ServerConfig,
    pub proxy: ProxyConfig,
    #[serde(default)]
    pub quic: QuicConfig,
    #[serde(default)]
    pub logging: LoggingConfig,
}

/// Server connection configuration
#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    /// Server address (host:port)
    pub address: String,
    /// Server name for TLS SNI (defaults to host from address)
    pub server_name: Option<String>,
    /// Skip TLS certificate verification (insecure, dev only)
    #[serde(default)]
    pub insecure: bool,
}

impl ServerConfig {
    /// Get the server name for TLS SNI
    pub fn get_server_name(&self) -> &str {
        self.server_name.as_deref().unwrap_or_else(|| {
            // Extract host from address (strip port)
            self.address
                .rsplit_once(':')
                .map(|(host, _)| host)
                .unwrap_or(&self.address)
        })
    }
}

/// Local proxy configuration
#[derive(Debug, Clone, Deserialize)]
pub struct ProxyConfig {
    /// SOCKS5 proxy bind address
    #[serde(default = "default_socks5_bind")]
    pub socks5_bind: SocketAddr,
    /// HTTP proxy bind address
    #[serde(default = "default_http_bind")]
    pub http_bind: SocketAddr,
    /// Enable SOCKS5 proxy
    #[serde(default = "default_true")]
    pub socks5_enabled: bool,
    /// Enable HTTP proxy
    #[serde(default = "default_true")]
    pub http_enabled: bool,
}

/// QUIC protocol configuration
#[derive(Debug, Clone, Deserialize)]
pub struct QuicConfig {
    /// Connection idle timeout in seconds
    #[serde(default = "default_idle_timeout")]
    pub idle_timeout_secs: u64,
    /// Enable 0-RTT for faster reconnection
    #[serde(default = "default_true")]
    pub enable_0rtt: bool,
    /// Maximum concurrent streams
    #[serde(default = "default_max_streams")]
    pub max_streams: u32,
}

impl Default for QuicConfig {
    fn default() -> Self {
        Self {
            idle_timeout_secs: default_idle_timeout(),
            enable_0rtt: default_true(),
            max_streams: default_max_streams(),
        }
    }
}

/// Logging configuration
#[derive(Debug, Clone, Deserialize)]
pub struct LoggingConfig {
    /// Log level: trace, debug, info, warn, error
    #[serde(default = "default_log_level")]
    pub level: String,
    /// Output format: "json" or "pretty"
    #[serde(default = "default_log_format")]
    pub format: String,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: default_log_level(),
            format: default_log_format(),
        }
    }
}

// Default value functions
fn default_socks5_bind() -> SocketAddr {
    "127.0.0.1:1080".parse().unwrap()
}

fn default_http_bind() -> SocketAddr {
    "127.0.0.1:8080".parse().unwrap()
}

fn default_true() -> bool {
    true
}

fn default_idle_timeout() -> u64 {
    30
}

fn default_max_streams() -> u32 {
    100
}

fn default_log_level() -> String {
    "info".to_string()
}

fn default_log_format() -> String {
    "pretty".to_string()
}

impl Config {
    /// Load configuration from a TOML file
    pub fn load(path: &Path) -> Result<Self> {
        let contents = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read config file: {:?}", path))?;

        let config: Config =
            toml::from_str(&contents).with_context(|| "Failed to parse config file")?;

        config.validate()?;
        Ok(config)
    }

    /// Validate configuration values
    fn validate(&self) -> Result<()> {
        if self.server.address.is_empty() {
            anyhow::bail!("server.address must not be empty");
        }
        if self.quic.idle_timeout_secs == 0 {
            anyhow::bail!("quic.idle_timeout_secs must be > 0");
        }
        if self.quic.max_streams == 0 {
            anyhow::bail!("quic.max_streams must be > 0");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_server_name_extraction() {
        let config = ServerConfig {
            address: "example.com:443".to_string(),
            server_name: None,
            insecure: false,
        };
        assert_eq!(config.get_server_name(), "example.com");

        let config_with_name = ServerConfig {
            address: "example.com:443".to_string(),
            server_name: Some("custom.example.com".to_string()),
            insecure: false,
        };
        assert_eq!(config_with_name.get_server_name(), "custom.example.com");
    }

    #[test]
    fn test_defaults() {
        let quic = QuicConfig::default();
        assert_eq!(quic.idle_timeout_secs, 30);
        assert!(quic.enable_0rtt);
        assert_eq!(quic.max_streams, 100);
    }
}

