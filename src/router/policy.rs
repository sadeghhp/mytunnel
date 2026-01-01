//! Routing policy
//!
//! Defines rules for routing decisions.

use super::dispatcher::Request;

/// Route decision
#[derive(Debug, Clone)]
pub enum RouteDecision {
    /// Allow the request to proceed
    Allow {
        /// Optional egress hint
        egress_hint: Option<String>,
    },
    /// Deny the request
    Deny {
        /// Reason for denial
        reason: String,
    },
    /// Rate limit exceeded
    RateLimited,
}

/// Routing policy configuration
#[derive(Debug, Clone)]
pub struct RoutingPolicy {
    /// Allow all traffic by default
    pub default_allow: bool,
    /// Blocked hosts (exact match)
    pub blocked_hosts: Vec<String>,
    /// Blocked ports
    pub blocked_ports: Vec<u16>,
    /// Allowed ports only (if not empty)
    pub allowed_ports: Vec<u16>,
}

impl Default for RoutingPolicy {
    fn default() -> Self {
        Self {
            default_allow: true,
            blocked_hosts: vec![],
            blocked_ports: vec![],
            allowed_ports: vec![], // Empty = all allowed
        }
    }
}

impl RoutingPolicy {
    /// Make a routing decision for a request
    pub fn decide(&self, request: &Request) -> RouteDecision {
        // Check blocked hosts
        if self.blocked_hosts.iter().any(|h| h == &request.target_host) {
            return RouteDecision::Deny {
                reason: "Host is blocked".to_string(),
            };
        }

        // Check blocked ports
        if self.blocked_ports.contains(&request.target_port) {
            return RouteDecision::Deny {
                reason: "Port is blocked".to_string(),
            };
        }

        // Check allowed ports (if specified)
        if !self.allowed_ports.is_empty() && !self.allowed_ports.contains(&request.target_port) {
            return RouteDecision::Deny {
                reason: "Port not in allowed list".to_string(),
            };
        }

        // Default decision
        if self.default_allow {
            RouteDecision::Allow { egress_hint: None }
        } else {
            RouteDecision::Deny {
                reason: "Default deny policy".to_string(),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::router::dispatcher::RequestType;

    fn make_request(host: &str, port: u16) -> Request {
        Request {
            request_type: RequestType::TcpConnect,
            target_host: host.to_string(),
            target_port: port,
            source_addr: "127.0.0.1:12345".parse().unwrap(),
        }
    }

    #[test]
    fn test_default_allow() {
        let policy = RoutingPolicy::default();
        let request = make_request("example.com", 443);
        
        assert!(matches!(policy.decide(&request), RouteDecision::Allow { .. }));
    }

    #[test]
    fn test_blocked_host() {
        let policy = RoutingPolicy {
            blocked_hosts: vec!["blocked.com".to_string()],
            ..Default::default()
        };

        let request = make_request("blocked.com", 443);
        assert!(matches!(policy.decide(&request), RouteDecision::Deny { .. }));

        let request = make_request("allowed.com", 443);
        assert!(matches!(policy.decide(&request), RouteDecision::Allow { .. }));
    }

    #[test]
    fn test_blocked_port() {
        let policy = RoutingPolicy {
            blocked_ports: vec![25], // Block SMTP
            ..Default::default()
        };

        let request = make_request("example.com", 25);
        assert!(matches!(policy.decide(&request), RouteDecision::Deny { .. }));

        let request = make_request("example.com", 443);
        assert!(matches!(policy.decide(&request), RouteDecision::Allow { .. }));
    }
}

