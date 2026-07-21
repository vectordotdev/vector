//! Minimal PROXY protocol v2 header parser.
//!
//! Parses the binary v2 header a fronting proxy (e.g. HAProxy `send-proxy-v2`)
//! prepends to a forwarded connection, exposing the original source/destination
//! address and any TLV fields (including custom `0xE0`..=`0xEF` values such as a
//! tenant identifier). See the PROXY protocol spec, section 2.2.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

use tokio::io::{AsyncRead, AsyncReadExt};
use vector_lib::config::LogNamespace;
use vector_lib::event::Event;
use vector_lib::lookup::PathPrefix;
use vrl::owned_value_path;
use vrl::path::OwnedValuePath;
use vrl::value::{ObjectMap, Value};

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

    /// Convert the parsed header into an event metadata object of the shape
    /// `{ version: 2, tlvs: { "0xe0": "<value>", .. } }`.
    ///
    /// TLV keys are the lowercase hex type byte (e.g. `0xe0`) since the wire
    /// format identifies fields by numeric type, not name. Values are exposed
    /// as UTF-8 strings when valid, otherwise as raw bytes.
    pub fn into_metadata(self) -> ObjectMap {
        let mut tlvs = ObjectMap::new();
        for tlv in self.tlvs {
            let key = format!("0x{:02x}", tlv.kind);
            let value = match String::from_utf8(tlv.value.clone()) {
                Ok(s) => Value::from(s),
                Err(_) => Value::from(tlv.value),
            };
            tlvs.insert(key.into(), value);
        }

        let mut map = ObjectMap::new();
        map.insert("version".into(), Value::from(2));
        map.insert("tlvs".into(), Value::from(tlvs));
        map
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

/// Inject parsed PROXY protocol metadata onto each log event in `events`.
///
/// Non-log events (metrics, traces produced by native codecs) are skipped
/// rather than coerced, since `Event::as_mut_log` panics on them.
pub fn inject_metadata(
    events: &mut [Event],
    metadata: &ObjectMap,
    log_namespace: LogNamespace,
    source_name: &'static str,
) {
    use vector_lib::config::LegacyKey;
    use vrl::path;

    for event in events {
        if let Event::Log(log) = event {
            log_namespace.insert_source_metadata(
                source_name,
                log,
                Some(LegacyKey::Overwrite(path!("proxy_protocol"))),
                path!("proxy_protocol"),
                metadata.clone(),
            );
        }
    }
}

/// The metadata path where PROXY protocol fields are inserted, for declaring
/// the source's output schema.
pub fn metadata_path() -> OwnedValuePath {
    owned_value_path!("proxy_protocol")
}

/// The legacy-namespace insertion prefix, exposed for schema declaration.
pub const LEGACY_PREFIX: PathPrefix = PathPrefix::Event;

/// Read and consume exactly one PROXY protocol v2 header from `reader`,
/// leaving the reader positioned at the first payload byte.
///
/// Reads the fixed 16-byte prefix, derives the declared length, then reads
/// exactly that many further bytes before parsing. Because it reads only the
/// header bytes, whatever follows on the stream is untouched and available to
/// the decoder.
pub async fn read_v2_header<R>(reader: &mut R) -> std::io::Result<ProxyHeader>
where
    R: AsyncRead + Unpin,
{
    let mut prefix = [0u8; V2_PREFIX_LEN];
    reader.read_exact(&mut prefix).await?;

    let total = v2_total_len(&prefix).ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "expected PROXY protocol v2 signature",
        )
    })?;

    let mut buf = vec![0u8; total];
    buf[..V2_PREFIX_LEN].copy_from_slice(&prefix);
    reader.read_exact(&mut buf[V2_PREFIX_LEN..]).await?;

    let (_, header) = parse_v2(&buf)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, format!("{e:?}")))?;
    Ok(header)
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

    /// Real bytes captured from `haproxy:2.9` configured with
    /// `send-proxy-v2 set-proxy-v2-tlv-fmt(0xE0) rajesh.com`, followed by a
    /// "HELLO-PAYLOAD\n" payload. Guards against a spec misreading shared by
    /// the hand-built fixtures above.
    /// The exact 55 bytes captured from haproxy:2.9 (header + payload).
    fn real_capture() -> [u8; 55] {
        [
            0x0d, 0x0a, 0x0d, 0x0a, 0x00, 0x0d, 0x0a, 0x51, 0x55, 0x49, 0x54, 0x0a, 0x21, 0x11,
            0x00, 0x19, 0xc0, 0xa8, 0x9b, 0x01, 0xc0, 0xa8, 0x9b, 0x03, 0xb4, 0x26, 0x1b, 0x58,
            0xe0, 0x00, 0x0a, 0x72, 0x61, 0x6a, 0x65, 0x73, 0x68, 0x2e, 0x63, 0x6f, 0x6d, 0x48,
            0x45, 0x4c, 0x4c, 0x4f, 0x2d, 0x50, 0x41, 0x59, 0x4c, 0x4f, 0x41, 0x44, 0x0a,
        ]
    }

    #[test]
    fn inject_metadata_skips_non_log_events() {
        use vector_lib::event::{Event, Metric, MetricKind, MetricValue};

        let value = b"rajesh.com";
        let mut tlv = vec![0xE0];
        tlv.extend_from_slice(&(value.len() as u16).to_be_bytes());
        tlv.extend_from_slice(value);
        let buf = v4_header(&tlv);
        let meta = parse_v2(&buf).unwrap().1.into_metadata();

        let mut events = vec![
            Event::Log(Default::default()),
            Event::Metric(Metric::new(
                "m",
                MetricKind::Absolute,
                MetricValue::Counter { value: 1.0 },
            )),
        ];

        // Must not panic on the metric event.
        inject_metadata(&mut events, &meta, LogNamespace::Legacy, "socket");

        // Log event carries the metadata.
        use vrl::event_path;
        let log = events[0].as_log();
        assert_eq!(
            log.get(event_path!("proxy_protocol"))
                .and_then(|v| v.as_object())
                .and_then(|o| o.get("tlvs"))
                .and_then(|v| v.as_object())
                .and_then(|o| o.get("0xe0")),
            Some(&Value::from("rajesh.com"))
        );
        // Metric event is untouched (still a metric).
        assert!(matches!(events[1], Event::Metric(_)));
    }

    #[tokio::test]
    async fn read_v2_header_consumes_only_the_header() {
        let wire = real_capture();
        let mut cursor = std::io::Cursor::new(wire.to_vec());
        let header = read_v2_header(&mut cursor).await.expect("header");
        assert_eq!(header.tlv(0xE0), Some(&b"rajesh.com"[..]));
        // Remaining bytes on the stream are exactly the payload.
        let mut rest = Vec::new();
        tokio::io::AsyncReadExt::read_to_end(&mut cursor, &mut rest)
            .await
            .unwrap();
        assert_eq!(rest, b"HELLO-PAYLOAD\n");
    }

    #[test]
    fn into_metadata_exposes_hex_keyed_tlvs() {
        let value = b"rajesh.com";
        let mut tlv = vec![0xE0];
        tlv.extend_from_slice(&(value.len() as u16).to_be_bytes());
        tlv.extend_from_slice(value);
        let buf = v4_header(&tlv);
        let (_, header) = parse_v2(&buf).expect("valid header");

        let meta = header.into_metadata();
        assert_eq!(meta.get("version"), Some(&Value::from(2)));
        let tlvs = match meta.get("tlvs") {
            Some(Value::Object(o)) => o,
            other => panic!("expected tlvs object, got {other:?}"),
        };
        assert_eq!(tlvs.get("0xe0"), Some(&Value::from("rajesh.com")));
    }

    #[tokio::test]
    async fn read_v2_header_errors_on_non_pp2_stream() {
        let mut cursor = std::io::Cursor::new(b"not a proxy header at all........".to_vec());
        assert!(read_v2_header(&mut cursor).await.is_err());
    }

    #[test]
    fn parses_real_haproxy_capture() {
        let wire: [u8; 55] = [
            0x0d, 0x0a, 0x0d, 0x0a, 0x00, 0x0d, 0x0a, 0x51, 0x55, 0x49, 0x54, 0x0a, // sig
            0x21, 0x11, 0x00, 0x19, // v2/PROXY, INET/STREAM, len 25
            0xc0, 0xa8, 0x9b, 0x01, // src 192.168.155.1
            0xc0, 0xa8, 0x9b, 0x03, // dst 192.168.155.3
            0xb4, 0x26, // src port 46118
            0x1b, 0x58, // dst port 7000
            0xe0, 0x00, 0x0a, // TLV 0xE0, len 10
            0x72, 0x61, 0x6a, 0x65, 0x73, 0x68, 0x2e, 0x63, 0x6f, 0x6d, // "rajesh.com"
            0x48, 0x45, 0x4c, 0x4c, 0x4f, 0x2d, 0x50, 0x41, 0x59, 0x4c, 0x4f, 0x41, 0x44,
            0x0a, // "HELLO-PAYLOAD\n" payload
        ];
        let (consumed, header) = parse_v2(&wire).expect("real haproxy header");
        assert_eq!(consumed, 41, "header is 16 prefix + 25 declared");
        assert_eq!(header.source, Some("192.168.155.1:46118".parse().unwrap()));
        assert_eq!(
            header.destination,
            Some("192.168.155.3:7000".parse().unwrap())
        );
        assert_eq!(header.tlv(0xE0), Some(&b"rajesh.com"[..]));
        // Payload survives untouched after the header.
        assert_eq!(&wire[consumed..], b"HELLO-PAYLOAD\n");
    }
}
