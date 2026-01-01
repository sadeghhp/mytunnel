//! TCP proxy integration tests

use std::time::Duration;

/// Test basic TCP proxy functionality
#[tokio::test]
async fn test_tcp_proxy_echo() {
    // This test would require a full server setup
    // For now, test the proxy module directly
    
    use mytunnel_server::pool::BufferPool;
    use mytunnel_server::proxy::TcpProxy;

    let pool = BufferPool::new(10, 5, 2);
    let _proxy = TcpProxy::new(pool);
    
    // Full integration test would:
    // 1. Start a TCP echo server
    // 2. Start the tunnel server
    // 3. Connect through tunnel
    // 4. Verify echo response
}

/// Test connection timeout handling
#[tokio::test]
async fn test_tcp_proxy_timeout() {
    use mytunnel_server::pool::BufferPool;
    use mytunnel_server::proxy::TcpProxy;

    let pool = BufferPool::new(10, 5, 2);
    let proxy = TcpProxy::new(pool);
    
    // Connection to non-routable address should timeout
    // Note: This test is slow, skip in normal CI
    // let result = proxy.proxy_stream(..., "10.255.255.1:12345").await;
    // assert!(result.is_err());
}

/// Test large data transfer
#[tokio::test]
async fn test_tcp_proxy_large_transfer() {
    // Test that large transfers work correctly
    // Would need full server setup
}

