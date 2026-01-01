//! HTTP API for connection monitoring
//!
//! Provides JSON endpoints for viewing connected users and server stats.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::Arc;
use std::thread;

use serde::Serialize;
use tracing::{debug, error, info, warn};

use crate::connection::ConnectionManager;
use super::counters::METRICS;

/// API response for /connections endpoint
#[derive(Serialize)]
struct ConnectionsResponse {
    count: usize,
    connections: Vec<crate::connection::ConnectionInfo>,
}

/// API response for /stats endpoint
#[derive(Serialize)]
struct StatsResponse {
    connections_total: u64,
    connections_active: u64,
    connections_failed: u64,
    bytes_received: u64,
    bytes_sent: u64,
    streams_opened: u64,
    streams_closed: u64,
    errors_total: u64,
}

/// Start the connections API server
///
/// This runs a simple HTTP server that responds to:
/// - GET /connections - List all active connections
/// - GET /stats - Server statistics
pub fn start_api_server(addr: SocketAddr, conn_manager: Arc<ConnectionManager>) {
    thread::spawn(move || {
        if let Err(e) = run_api_server(addr, conn_manager) {
            error!(error = %e, "API server error");
        }
    });
    info!(%addr, "Connections API server started");
}

fn run_api_server(addr: SocketAddr, conn_manager: Arc<ConnectionManager>) -> std::io::Result<()> {
    let listener = TcpListener::bind(addr)?;
    
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let conn_manager = conn_manager.clone();
                thread::spawn(move || {
                    if let Err(e) = handle_request(stream, &conn_manager) {
                        debug!(error = %e, "Request handling error");
                    }
                });
            }
            Err(e) => {
                warn!(error = %e, "Failed to accept connection");
            }
        }
    }
    
    Ok(())
}

fn handle_request(mut stream: TcpStream, conn_manager: &ConnectionManager) -> std::io::Result<()> {
    let mut buffer = [0u8; 1024];
    let n = stream.read(&mut buffer)?;
    
    if n == 0 {
        return Ok(());
    }
    
    let request = String::from_utf8_lossy(&buffer[..n]);
    let first_line = request.lines().next().unwrap_or("");
    
    // Parse request path
    let path = first_line
        .split_whitespace()
        .nth(1)
        .unwrap_or("/");
    
    let (status, body) = match path {
        "/connections" => {
            let connections = conn_manager.list_connections();
            let response = ConnectionsResponse {
                count: connections.len(),
                connections,
            };
            ("200 OK", serde_json::to_string_pretty(&response).unwrap_or_default())
        }
        "/stats" => {
            let snapshot = METRICS.snapshot();
            let response = StatsResponse {
                connections_total: snapshot.connections_total,
                connections_active: snapshot.connections_active,
                connections_failed: snapshot.connections_failed,
                bytes_received: snapshot.bytes_received,
                bytes_sent: snapshot.bytes_sent,
                streams_opened: snapshot.streams_opened,
                streams_closed: snapshot.streams_closed,
                errors_total: snapshot.errors_total,
            };
            ("200 OK", serde_json::to_string_pretty(&response).unwrap_or_default())
        }
        "/" => {
            let help = r#"{
  "endpoints": {
    "/connections": "List all active connections",
    "/stats": "Server statistics"
  }
}"#;
            ("200 OK", help.to_string())
        }
        _ => {
            ("404 Not Found", r#"{"error": "Not found"}"#.to_string())
        }
    };
    
    let response = format!(
        "HTTP/1.1 {}\r\n\
         Content-Type: application/json\r\n\
         Content-Length: {}\r\n\
         Access-Control-Allow-Origin: *\r\n\
         Connection: close\r\n\
         \r\n\
         {}",
        status,
        body.len(),
        body
    );
    
    stream.write_all(response.as_bytes())?;
    stream.flush()?;
    
    Ok(())
}

