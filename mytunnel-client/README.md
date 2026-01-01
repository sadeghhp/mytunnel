# MyTunnel Client

QUIC-based tunnel client with SOCKS5 and HTTP proxy support.

## Features

- **QUIC Transport**: Secure, efficient tunnel over QUIC protocol
- **SOCKS5 Proxy**: Full SOCKS5 support including UDP ASSOCIATE
- **HTTP CONNECT Proxy**: Standard HTTP tunneling proxy
- **Auto-Reconnect**: Automatic reconnection on connection loss
- **Cross-Platform**: Works on Linux, macOS, and Windows

## Quick Start

```bash
# Build the client
cd mytunnel-client
cargo build --release

# Copy and edit configuration
cp client-config.example.toml client-config.toml
# Edit client-config.toml with your server address

# Test connection to server
./target/release/mytunnel-client test-connection -c client-config.toml

# Run the client
./target/release/mytunnel-client run -c client-config.toml
```

## Configuration

See `client-config.example.toml` for all available options.

### Minimal Configuration

```toml
[server]
address = "your-server.com:443"

[proxy]
socks5_bind = "127.0.0.1:1080"
http_bind = "127.0.0.1:8080"
```

### Development Configuration (Self-Signed Certs)

```toml
[server]
address = "localhost:4433"
insecure = true  # Skip cert verification

[proxy]
socks5_bind = "127.0.0.1:1080"
http_bind = "127.0.0.1:8080"
```

## Usage

### Using SOCKS5 Proxy

```bash
# curl with SOCKS5
curl --socks5 127.0.0.1:1080 https://example.com

# Configure browser to use SOCKS5 proxy at 127.0.0.1:1080
```

### Using HTTP Proxy

```bash
# curl with HTTP proxy
curl -x http://127.0.0.1:8080 https://example.com

# Or set environment variable
export https_proxy=http://127.0.0.1:8080
curl https://example.com
```

## Commands

### run

Start the tunnel client with local proxy servers.

```bash
mytunnel-client run -c config.toml
```

### test-connection

Test connectivity to the tunnel server.

```bash
mytunnel-client test-connection -c config.toml
```

## Protocol

The client implements the MyTunnel protocol:

### TCP Tunneling (QUIC Streams)

```
Request:  [0x01][Port:2B][HostLen:1B][Host]
Response: [Status:1B] (0x00=OK)
```

### UDP Relay (QUIC Datagrams)

```
Packet: [Port:2B][HostLen:1B][Host][Payload]
```

## Building

```bash
# Debug build
cargo build

# Release build (optimized)
cargo build --release

# Run tests
cargo test
```

## License

MIT

