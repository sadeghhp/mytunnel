//! UDP relay integration tests

use std::net::SocketAddr;

/// Test basic UDP relay functionality
#[tokio::test]
async fn test_udp_relay_dns() {
    use mytunnel_server::pool::BufferPool;
    use mytunnel_server::proxy::UdpRelay;

    let pool = BufferPool::new(10, 5, 2);
    let relay = UdpRelay::new(pool);

    // Test DNS query through relay (requires network)
    // Skip in CI without network access
    if std::env::var("TEST_WITH_NETWORK").is_ok() {
        // Simple DNS query for google.com A record
        let dns_query = build_dns_query("google.com");
        let result = relay.relay_packet("8.8.8.8:53", &dns_query).await;
        
        assert!(result.is_ok());
        let response = result.unwrap();
        assert!(response.len() > 12); // DNS header is 12 bytes
    }
}

/// Test UDP relay timeout
#[tokio::test]
async fn test_udp_relay_timeout() {
    use mytunnel_server::pool::BufferPool;
    use mytunnel_server::proxy::UdpRelay;

    let pool = BufferPool::new(10, 5, 2);
    let relay = UdpRelay::new(pool);

    // Send to non-responsive address
    let result = relay.relay_packet("10.255.255.1:12345", b"test").await;
    
    // Should timeout
    assert!(result.is_err());
}

/// Build a simple DNS query packet
fn build_dns_query(domain: &str) -> Vec<u8> {
    let mut query = Vec::new();
    
    // Transaction ID
    query.extend_from_slice(&[0x12, 0x34]);
    // Flags (standard query)
    query.extend_from_slice(&[0x01, 0x00]);
    // Questions: 1
    query.extend_from_slice(&[0x00, 0x01]);
    // Answers: 0
    query.extend_from_slice(&[0x00, 0x00]);
    // Authority: 0
    query.extend_from_slice(&[0x00, 0x00]);
    // Additional: 0
    query.extend_from_slice(&[0x00, 0x00]);
    
    // Domain name
    for part in domain.split('.') {
        query.push(part.len() as u8);
        query.extend_from_slice(part.as_bytes());
    }
    query.push(0); // Null terminator
    
    // Type: A (1)
    query.extend_from_slice(&[0x00, 0x01]);
    // Class: IN (1)
    query.extend_from_slice(&[0x00, 0x01]);
    
    query
}

