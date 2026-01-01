//! Local proxy servers
//!
//! Provides SOCKS5 and HTTP CONNECT proxy interfaces.

pub mod http;
pub mod socks5;

pub use http::HttpProxy;
pub use socks5::Socks5Proxy;

