//! Wire protocol encoding/decoding
//!
//! Implements the tunnel protocol matching the server format:
//! - TCP Tunnel Request: [Type(1)][Port(2)][HostLen(1)][Host(N)]
//! - UDP Relay: [Port(2)][HostLen(1)][Host(N)][Payload]

use anyhow::{bail, Result};
use bytes::{Buf, BufMut, Bytes, BytesMut};

/// Request types for TCP tunneling
pub const TCP_CONNECT: u8 = 0x01;

/// Response status codes
pub const STATUS_OK: u8 = 0x00;
pub const STATUS_ERROR: u8 = 0xFF;

/// Encode a TCP tunnel request
///
/// Format: [Type(1)][Port(2 BE)][HostLen(1)][Host(N)]
pub fn encode_tcp_request(host: &str, port: u16) -> Result<Vec<u8>> {
    let host_bytes = host.as_bytes();
    if host_bytes.len() > 255 {
        bail!("Host name too long (max 255 bytes)");
    }

    let mut buf = Vec::with_capacity(4 + host_bytes.len());
    buf.push(TCP_CONNECT);
    buf.put_u16(port);
    buf.push(host_bytes.len() as u8);
    buf.extend_from_slice(host_bytes);

    Ok(buf)
}

/// Decode a TCP tunnel response
///
/// Format: [Status(1)]
/// Returns Ok(()) if status is OK, Err otherwise
pub fn decode_tcp_response(data: &[u8]) -> Result<()> {
    if data.is_empty() {
        bail!("Empty response");
    }

    match data[0] {
        STATUS_OK => Ok(()),
        STATUS_ERROR => bail!("Server returned error"),
        status => bail!("Unknown status code: {}", status),
    }
}

/// Encode a UDP datagram for relay
///
/// Format: [Port(2 BE)][HostLen(1)][Host(N)][Payload]
pub fn encode_udp_packet(host: &str, port: u16, payload: &[u8]) -> Result<Vec<u8>> {
    let host_bytes = host.as_bytes();
    if host_bytes.len() > 255 {
        bail!("Host name too long (max 255 bytes)");
    }

    let mut buf = Vec::with_capacity(3 + host_bytes.len() + payload.len());
    buf.put_u16(port);
    buf.push(host_bytes.len() as u8);
    buf.extend_from_slice(host_bytes);
    buf.extend_from_slice(payload);

    Ok(buf)
}

/// Decoded UDP packet
#[derive(Debug)]
pub struct UdpPacket {
    pub host: String,
    pub port: u16,
    pub payload: Bytes,
}

/// Decode a UDP datagram response
///
/// Format: [Port(2 BE)][HostLen(1)][Host(N)][Payload]
pub fn decode_udp_packet(data: Bytes) -> Result<UdpPacket> {
    if data.len() < 4 {
        bail!("UDP packet too short");
    }

    let mut buf = data;
    let port = buf.get_u16();
    let host_len = buf.get_u8() as usize;

    if buf.remaining() < host_len {
        bail!("UDP packet truncated: expected {} host bytes", host_len);
    }

    let host = String::from_utf8(buf.copy_to_bytes(host_len).to_vec())?;
    let payload = buf;

    Ok(UdpPacket {
        host,
        port,
        payload,
    })
}

/// SOCKS5 protocol constants and helpers
pub mod socks5 {
    /// SOCKS5 version
    pub const VERSION: u8 = 0x05;

    /// Authentication methods
    pub const AUTH_NONE: u8 = 0x00;
    pub const AUTH_USERPASS: u8 = 0x02;
    pub const AUTH_NO_ACCEPTABLE: u8 = 0xFF;

    /// Commands
    pub const CMD_CONNECT: u8 = 0x01;
    pub const CMD_BIND: u8 = 0x02;
    pub const CMD_UDP_ASSOCIATE: u8 = 0x03;

    /// Address types
    pub const ATYP_IPV4: u8 = 0x01;
    pub const ATYP_DOMAIN: u8 = 0x03;
    pub const ATYP_IPV6: u8 = 0x04;

    /// Reply codes
    pub const REP_SUCCESS: u8 = 0x00;
    pub const REP_GENERAL_FAILURE: u8 = 0x01;
    pub const REP_CONN_NOT_ALLOWED: u8 = 0x02;
    pub const REP_NETWORK_UNREACHABLE: u8 = 0x03;
    pub const REP_HOST_UNREACHABLE: u8 = 0x04;
    pub const REP_CONN_REFUSED: u8 = 0x05;
    pub const REP_TTL_EXPIRED: u8 = 0x06;
    pub const REP_CMD_NOT_SUPPORTED: u8 = 0x07;
    pub const REP_ATYP_NOT_SUPPORTED: u8 = 0x08;

    use super::*;
    use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4};

    /// Parse SOCKS5 address from buffer
    pub fn parse_address(data: &mut BytesMut) -> Result<(String, u16)> {
        if data.is_empty() {
            bail!("Empty address data");
        }

        let atyp = data.get_u8();

        let host = match atyp {
            ATYP_IPV4 => {
                if data.remaining() < 4 {
                    bail!("Truncated IPv4 address");
                }
                let mut octets = [0u8; 4];
                data.copy_to_slice(&mut octets);
                Ipv4Addr::from(octets).to_string()
            }
            ATYP_DOMAIN => {
                if data.is_empty() {
                    bail!("Missing domain length");
                }
                let len = data.get_u8() as usize;
                if data.remaining() < len {
                    bail!("Truncated domain name");
                }
                let domain = data.copy_to_bytes(len);
                String::from_utf8(domain.to_vec())?
            }
            ATYP_IPV6 => {
                if data.remaining() < 16 {
                    bail!("Truncated IPv6 address");
                }
                let mut octets = [0u8; 16];
                data.copy_to_slice(&mut octets);
                Ipv6Addr::from(octets).to_string()
            }
            _ => bail!("Unknown address type: {}", atyp),
        };

        if data.remaining() < 2 {
            bail!("Missing port");
        }
        let port = data.get_u16();

        Ok((host, port))
    }

    /// Encode SOCKS5 reply
    pub fn encode_reply(status: u8, bind_addr: SocketAddr) -> Vec<u8> {
        let mut buf = Vec::with_capacity(22);
        buf.push(VERSION);
        buf.push(status);
        buf.push(0x00); // Reserved

        match bind_addr {
            SocketAddr::V4(addr) => {
                buf.push(ATYP_IPV4);
                buf.extend_from_slice(&addr.ip().octets());
                buf.put_u16(addr.port());
            }
            SocketAddr::V6(addr) => {
                buf.push(ATYP_IPV6);
                buf.extend_from_slice(&addr.ip().octets());
                buf.put_u16(addr.port());
            }
        }

        buf
    }

    /// Create a "zero" bind address for replies where we don't have a real address
    pub fn zero_bind_addr_v4() -> SocketAddr {
        SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_tcp_request() {
        let req = encode_tcp_request("example.com", 443).unwrap();
        assert_eq!(req[0], TCP_CONNECT);
        assert_eq!(u16::from_be_bytes([req[1], req[2]]), 443);
        assert_eq!(req[3], 11); // "example.com".len()
        assert_eq!(&req[4..], b"example.com");
    }

    #[test]
    fn test_decode_tcp_response() {
        assert!(decode_tcp_response(&[STATUS_OK]).is_ok());
        assert!(decode_tcp_response(&[STATUS_ERROR]).is_err());
        assert!(decode_tcp_response(&[]).is_err());
    }

    #[test]
    fn test_encode_udp_packet() {
        let packet = encode_udp_packet("dns.google", 53, b"test").unwrap();
        assert_eq!(u16::from_be_bytes([packet[0], packet[1]]), 53);
        assert_eq!(packet[2], 10); // "dns.google".len()
        assert_eq!(&packet[3..13], b"dns.google");
        assert_eq!(&packet[13..], b"test");
    }

    #[test]
    fn test_decode_udp_packet() {
        let data = encode_udp_packet("test.com", 8080, b"payload").unwrap();
        let packet = decode_udp_packet(Bytes::from(data)).unwrap();
        assert_eq!(packet.host, "test.com");
        assert_eq!(packet.port, 8080);
        assert_eq!(&packet.payload[..], b"payload");
    }
}

