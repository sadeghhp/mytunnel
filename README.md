# MyTunnel Server


High-performance QUIC-based tunnel server with zero-copy forwarding.

## Features

- **QUIC Transport**: Modern, secure, and efficient transport layer
- **Zero-Copy TCP Proxy**: Uses `splice()` on Linux for kernel-level forwarding
- **Batched UDP Relay**: Uses `sendmmsg()` for efficient multi-packet sending
- **Connection Migration**: Seamless handoff between networks (Wi-Fi to LTE)
- **Lock-Free Data Structures**: Minimal contention in hot paths
- **Pre-allocated Memory Pools**: No allocations during request handling
- **Full Observability**: Prometheus metrics, structured JSON logging, tracing

## Requirements

- Rust 1.75+
- Linux 5.6+ (for full performance, io_uring support)
- OpenSSL or equivalent for TLS

## Quick Start

```bash
# Build release version
cargo build --release

# Copy and edit configuration
cp config.example.toml config.toml
# Edit config.toml with your settings

# Run the server
./target/release/mytunnel-server config.toml
```

## Configuration

See `config.example.toml` for all available options.

### Key Settings

```toml
[server]
bind_addr = "0.0.0.0:443"
workers = 0  # 0 = auto-detect CPU cores

[quic]
max_connections = 100000
idle_timeout_secs = 30

[tls]
cert_path = "/etc/mytunnel/cert.pem"
key_path = "/etc/mytunnel/key.pem"
auto_generate = true  # Dev only

[metrics]
enabled = true
bind_addr = "127.0.0.1:9090"
```

## Performance Tuning

### System Configuration

Apply these sysctl settings for optimal performance:

```bash
# /etc/sysctl.conf
net.core.rmem_max = 134217728
net.core.wmem_max = 134217728
net.core.default_qdisc = fq
net.ipv4.tcp_congestion_control = bbr
fs.file-max = 2097152
```

### Process Limits

```bash
# /etc/security/limits.conf
* soft nofile 1048576
* hard nofile 1048576
```

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    Client Connections                        │
│                    (QUIC over UDP:443)                       │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                    QUIC Listeners                            │
│              (SO_REUSEPORT multi-core)                       │
└─────────────────────────────────────────────────────────────┘
                              │
              ┌───────────────┼───────────────┐
              ▼               ▼               ▼
        ┌──────────┐   ┌──────────┐   ┌──────────┐
        │ Streams  │   │ Datagrams│   │ Control  │
        │ (TCP)    │   │ (UDP)    │   │ Channel  │
        └────┬─────┘   └────┬─────┘   └──────────┘
             │              │
             ▼              ▼
        ┌──────────┐   ┌──────────┐
        │ TCP      │   │ UDP      │
        │ Proxy    │   │ Relay    │
        │ (splice) │   │(sendmmsg)│
        └────┬─────┘   └────┬─────┘
             │              │
             ▼              ▼
┌─────────────────────────────────────────────────────────────┐
│                    Target Servers                            │
└─────────────────────────────────────────────────────────────┘
```

## Metrics

Access Prometheus metrics at `http://localhost:9090/metrics`:

- `mytunnel_connections_total` - Total connections received
- `mytunnel_connections_active` - Currently active connections
- `mytunnel_bytes_received` - Total bytes received
- `mytunnel_bytes_sent` - Total bytes sent
- `mytunnel_streams_opened` - Total streams opened
- `mytunnel_datagrams_received` - Total datagrams received

## Protocol

### TCP Tunnel Request (Stream)

```
Request Header:
┌──────────┬──────────┬──────────┬──────────────┐
│ Type (1) │ Port (2) │ HostLen  │ Host (N)     │
│  0x01    │ BE u16   │ (1 byte) │ UTF-8 string │
└──────────┴──────────┴──────────┴──────────────┘

Response:
┌──────────┐
│ Status   │
│ 0x00=OK  │
└──────────┘

Then bidirectional data flow.
```

### UDP Relay (Datagram)

```
Datagram Format:
┌──────────┬──────────┬──────────────┬─────────┐
│ Port (2) │ HostLen  │ Host (N)     │ Payload │
│ BE u16   │ (1 byte) │ UTF-8 string │ bytes   │
└──────────┴──────────┴──────────────┴─────────┘
```

## Development

```bash
# Run tests
cargo test

# Run benchmarks
cargo bench

# Check code
cargo clippy

# Format code
cargo fmt
```

## License

MIT

