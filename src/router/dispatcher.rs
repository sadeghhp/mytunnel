//! Request dispatcher
//!
//! Routes incoming requests to appropriate handlers.

use std::net::SocketAddr;

use super::policy::{RouteDecision, RoutingPolicy};

/// Request types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestType {
    /// TCP connection request
    TcpConnect,
    /// UDP datagram relay
    UdpRelay,
    /// DNS query
    DnsQuery,
}

/// A request to be routed
#[derive(Debug)]
pub struct Request {
    /// Type of request
    pub request_type: RequestType,
    /// Target host
    pub target_host: String,
    /// Target port
    pub target_port: u16,
    /// Source address
    pub source_addr: SocketAddr,
}

/// Routes requests based on policy
pub struct RequestRouter {
    policy: RoutingPolicy,
}

impl RequestRouter {
    /// Create a new router with default policy
    pub fn new() -> Self {
        Self {
            policy: RoutingPolicy::default(),
        }
    }

    /// Create router with custom policy
    pub fn with_policy(policy: RoutingPolicy) -> Self {
        Self { policy }
    }

    /// Route a request
    pub fn route(&self, request: &Request) -> RouteDecision {
        self.policy.decide(request)
    }

    /// Check if target is allowed
    pub fn is_allowed(&self, request: &Request) -> bool {
        matches!(self.route(request), RouteDecision::Allow { .. })
    }
}

impl Default for RequestRouter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_routing() {
        let router = RequestRouter::new();
        
        let request = Request {
            request_type: RequestType::TcpConnect,
            target_host: "example.com".to_string(),
            target_port: 443,
            source_addr: "127.0.0.1:12345".parse().unwrap(),
        };

        assert!(router.is_allowed(&request));
    }
}

