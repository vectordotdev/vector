//! Minimal PROXY protocol v2 header parser.
//!
//! Parses the binary v2 header a fronting proxy (e.g. HAProxy `send-proxy-v2`)
//! prepends to a forwarded connection, exposing the original source/destination
//! address and any TLV fields (including custom `0xE0`..=`0xEF` values such as a
//! tenant identifier). See the PROXY protocol spec, section 2.2.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

/// The 12-byte signature that begins every PROXY protocol v2 header.
pub const V2_SIGNATURE: [u8; 12] = [
    0x0D, 0x0A, 0x0D, 0x0A, 0x00, 0x0D, 0x0A, 0x51, 0x55, 0x49, 0x54, 0x0A,
];

/// The fixed prefix length that must be read before the declared length is known.
pub const V2_PREFIX_LEN: usize = 16;

/// Errors produced while parsing a PROXY protocol v2 header.
#[derive(Debug, PartialEq, Eq)]
pub enum ParseError {
    /// The buffer is shorter than the bytes the header claims to contain.
    Truncated,
    /// The 12-byte v2 signature was not found at the start of the buffer.
    BadSignature,
    /// The version nibble was not `2`.
    UnsupportedVersion(u8),
    /// The address family/transport byte was not understood.
    UnsupportedFamily(u8),
}

/// A single Type-Length-Value entry from the v2 TLV vector.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Tlv {
    /// The TLV type byte.
    pub kind: u8,
    /// The raw TLV value bytes.
    pub value: Vec<u8>,
}

/// A decoded PROXY protocol v2 header.
#[derive(Debug, PartialEq, Eq, Clone, Default)]
pub struct ProxyHeader {
    /// Original client source address, if the family carried one.
    pub source: Option<SocketAddr>,
    /// Original destination address, if the family carried one.
    pub destination: Option<SocketAddr>,
    /// All TLV entries found after the address block, in order.
    pub tlvs: Vec<Tlv>,
}

impl ProxyHeader {
    /// Return the value of the first TLV matching `kind`, if present.
    pub fn tlv(&self, kind: u8) -> Option<&[u8]> {
        self.tlvs
            .iter()
            .find(|t| t.kind == kind)
            .map(|t| t.value.as_slice())
    }
}

/// Total number of bytes a complete v2 header occupies given its fixed prefix.
///
/// Returns `None` if fewer than [`V2_PREFIX_LEN`] bytes are available or the
/// signature does not match, so the caller knows not to treat the bytes as a
/// header.
pub fn v2_total_len(prefix: &[u8]) -> Option<usize> {
    if prefix.len() < V2_PREFIX_LEN || prefix[..12] != V2_SIGNATURE {
        return None;
    }
    let declared = u16::from_be_bytes([prefix[14], prefix[15]]) as usize;
    Some(V2_PREFIX_LEN + declared)
}

/// Parse a complete PROXY protocol v2 header from the start of `buf`.
///
/// `buf` must contain at least the full header (see [`v2_total_len`]); any
/// trailing payload bytes are ignored. Returns the decoded header and the
/// number of bytes consumed.
pub fn parse_v2(buf: &[u8]) -> Result<(usize, ProxyHeader), ParseError> {
    if buf.len() < V2_PREFIX_LEN {
        return Err(ParseError::Truncated);
    }
    if buf[..12] != V2_SIGNATURE {
        return Err(ParseError::BadSignature);
    }

    let ver = buf[12] >> 4;
    if ver != 2 {
        return Err(ParseError::UnsupportedVersion(ver));
    }
    // Lower nibble of byte 12 is the command (LOCAL/PROXY); not needed for the
    // minimal slice.

    let family = buf[13] >> 4;
    let declared = u16::from_be_bytes([buf[14], buf[15]]) as usize;
    let header_end = V2_PREFIX_LEN + declared;
    if buf.len() < header_end {
        return Err(ParseError::Truncated);
    }

    let addr_bytes = &buf[V2_PREFIX_LEN..header_end];
    let (source, destination, addr_len) = match family {
        // AF_INET
        0x1 => {
            if addr_bytes.len() < 12 {
                return Err(ParseError::Truncated);
            }
            let src_ip = Ipv4Addr::new(addr_bytes[0], addr_bytes[1], addr_bytes[2], addr_bytes[3]);
            let dst_ip = Ipv4Addr::new(addr_bytes[4], addr_bytes[5], addr_bytes[6], addr_bytes[7]);
            let src_port = u16::from_be_bytes([addr_bytes[8], addr_bytes[9]]);
            let dst_port = u16::from_be_bytes([addr_bytes[10], addr_bytes[11]]);
            (
                Some(SocketAddr::new(IpAddr::V4(src_ip), src_port)),
                Some(SocketAddr::new(IpAddr::V4(dst_ip), dst_port)),
                12,
            )
        }
        // AF_INET6
        0x2 => {
            if addr_bytes.len() < 36 {
                return Err(ParseError::Truncated);
            }
            let mut src = [0u8; 16];
            let mut dst = [0u8; 16];
            src.copy_from_slice(&addr_bytes[0..16]);
            dst.copy_from_slice(&addr_bytes[16..32]);
            let src_port = u16::from_be_bytes([addr_bytes[32], addr_bytes[33]]);
            let dst_port = u16::from_be_bytes([addr_bytes[34], addr_bytes[35]]);
            (
                Some(SocketAddr::new(IpAddr::V6(Ipv6Addr::from(src)), src_port)),
                Some(SocketAddr::new(IpAddr::V6(Ipv6Addr::from(dst)), dst_port)),
                36,
            )
        }
        // AF_UNSPEC (0x0) or AF_UNIX (0x3): no IP addresses to expose.
        0x0 | 0x3 => (None, None, if family == 0x3 { 216 } else { 0 }),
        other => return Err(ParseError::UnsupportedFamily(other)),
    };

    let tlvs = parse_tlvs(&addr_bytes[addr_len.min(addr_bytes.len())..]);

    Ok((
        header_end,
        ProxyHeader {
            source,
            destination,
            tlvs,
        },
    ))
}

/// Walk a TLV vector, skipping any entry whose declared length overruns the
/// buffer. Unknown types are retained as-is so callers can decide what to do.
fn parse_tlvs(mut buf: &[u8]) -> Vec<Tlv> {
    let mut out = Vec::new();
    while buf.len() >= 3 {
        let kind = buf[0];
        let len = u16::from_be_bytes([buf[1], buf[2]]) as usize;
        let value_start = 3;
        let value_end = value_start + len;
        if value_end > buf.len() {
            // Malformed length; stop rather than read out of bounds.
            break;
        }
        out.push(Tlv {
            kind,
            value: buf[value_start..value_end].to_vec(),
        });
        buf = &buf[value_end..];
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a v2 IPv4 header with the given TLV bytes appended.
    fn v4_header(tlvs: &[u8]) -> Vec<u8> {
        let addr: [u8; 12] = [
            1, 2, 3, 4, // src 1.2.3.4
            10, 0, 0, 1, // dst 10.0.0.1
            0xD4, 0x31, // src port 54321
            0x01, 0xBB, // dst port 443
        ];
        let len = (addr.len() + tlvs.len()) as u16;
        let mut buf = Vec::new();
        buf.extend_from_slice(&V2_SIGNATURE);
        buf.push(0x21); // ver 2, cmd PROXY
        buf.push(0x11); // AF_INET, STREAM
        buf.extend_from_slice(&len.to_be_bytes());
        buf.extend_from_slice(&addr);
        buf.extend_from_slice(tlvs);
        buf
    }

    #[test]
    fn rejects_bad_signature() {
        let buf = [0u8; 16];
        assert_eq!(parse_v2(&buf), Err(ParseError::BadSignature));
    }

    #[test]
    fn rejects_truncated_prefix() {
        let buf = [0x0D, 0x0A, 0x0D];
        assert_eq!(parse_v2(&buf), Err(ParseError::Truncated));
    }

    #[test]
    fn decodes_ipv4_addresses_and_ports() {
        let buf = v4_header(&[]);
        let (consumed, header) = parse_v2(&buf).expect("valid header");
        assert_eq!(consumed, buf.len());
        assert_eq!(header.source, Some("1.2.3.4:54321".parse().unwrap()));
        assert_eq!(header.destination, Some("10.0.0.1:443".parse().unwrap()));
        assert!(header.tlvs.is_empty());
    }

    #[test]
    fn decodes_ipv6_addresses() {
        let mut addr = Vec::new();
        addr.extend_from_slice(&Ipv6Addr::LOCALHOST.octets());
        addr.extend_from_slice(&Ipv6Addr::LOCALHOST.octets());
        addr.extend_from_slice(&1234u16.to_be_bytes());
        addr.extend_from_slice(&443u16.to_be_bytes());
        let mut buf = Vec::new();
        buf.extend_from_slice(&V2_SIGNATURE);
        buf.push(0x21);
        buf.push(0x21); // AF_INET6, STREAM
        buf.extend_from_slice(&(addr.len() as u16).to_be_bytes());
        buf.extend_from_slice(&addr);
        let (_, header) = parse_v2(&buf).expect("valid header");
        assert_eq!(header.source, Some("[::1]:1234".parse().unwrap()));
    }

    #[test]
    fn extracts_custom_tenant_tlv() {
        // TLV type 0xE0, value "rajesh.com"
        let value = b"rajesh.com";
        let mut tlv = vec![0xE0];
        tlv.extend_from_slice(&(value.len() as u16).to_be_bytes());
        tlv.extend_from_slice(value);
        let buf = v4_header(&tlv);
        let (_, header) = parse_v2(&buf).expect("valid header");
        assert_eq!(header.tlv(0xE0), Some(&b"rajesh.com"[..]));
    }

    #[test]
    fn skips_unknown_ssl_container_tlv_without_failing() {
        // A 0x20 SSL container TLV we do not decode; must be tolerated.
        let inner = [0x01, 0x00, 0x00, 0x00, 0x00]; // client byte + verify u32
        let mut tlv = vec![0x20];
        tlv.extend_from_slice(&(inner.len() as u16).to_be_bytes());
        tlv.extend_from_slice(&inner);
        // Followed by our custom tenant TLV.
        let value = b"acme";
        tlv.push(0xE0);
        tlv.extend_from_slice(&(value.len() as u16).to_be_bytes());
        tlv.extend_from_slice(value);

        let buf = v4_header(&tlv);
        let (_, header) = parse_v2(&buf).expect("valid header");
        assert_eq!(header.tlv(0x20).map(<[u8]>::to_vec), Some(inner.to_vec()));
        assert_eq!(header.tlv(0xE0), Some(&b"acme"[..]));
    }

    #[test]
    fn truncated_body_is_reported() {
        let mut buf = v4_header(&[]);
        buf.truncate(buf.len() - 2); // drop part of the address block
        assert_eq!(parse_v2(&buf), Err(ParseError::Truncated));
    }

    #[test]
    fn v2_total_len_reads_declared_length() {
        let buf = v4_header(&[]);
        assert_eq!(v2_total_len(&buf[..V2_PREFIX_LEN]), Some(buf.len()));
    }

    #[test]
    fn v2_total_len_rejects_non_signature() {
        let buf = [0u8; 16];
        assert_eq!(v2_total_len(&buf), None);
    }
}
