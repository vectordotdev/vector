//! Parser for NetFlow version 5 export datagrams (Cisco NetFlow Services Export).
//!
//! A UDP payload starts with a 24-byte header followed by zero or more fixed-width,
//! 48-byte IPv4 flow records.

use crate::sources::netflow::fields::FieldParser;

use dashmap::DashMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use tracing::{debug, warn};
use vector_lib::event::{Event, LogEvent};

/// Constants for NetFlow v5 datagram layout.
const NETFLOW_V5_VERSION: u16 = 5;
const NETFLOW_V5_HEADER_SIZE: usize = 24;
const NETFLOW_V5_RECORD_SIZE: usize = 48;
const MAX_FLOW_COUNT: u16 = 1000; // Sanity check for flow count

/// Parsed NetFlow v5 packet header.
#[derive(Debug, Clone)]
pub struct NetflowV5Header {
    pub version: u16,
    pub count: u16,
    pub sys_uptime: u32,
    pub unix_secs: u32,
    pub unix_nsecs: u32,
    pub flow_sequence: u32,
    pub engine_type: u8,
    pub engine_id: u8,
    pub sampling_interval: u16,
}

impl NetflowV5Header {
    /// Builds a header from the start of `data`.
    pub fn from_bytes(data: &[u8]) -> Result<Self, String> {
        if data.len() < NETFLOW_V5_HEADER_SIZE {
            return Err(format!(
                "Packet too short for NetFlow v5 header: {} bytes, need {}",
                data.len(),
                NETFLOW_V5_HEADER_SIZE
            ));
        }

        let version = u16::from_be_bytes([data[0], data[1]]);
        if version != NETFLOW_V5_VERSION {
            return Err(format!(
                "Invalid NetFlow v5 version: {}, expected {}",
                version, NETFLOW_V5_VERSION
            ));
        }

        let count = u16::from_be_bytes([data[2], data[3]]);
        if count > MAX_FLOW_COUNT {
            return Err(format!(
                "Unreasonable flow count: {}, maximum expected {}",
                count, MAX_FLOW_COUNT
            ));
        }

        // Validate packet length matches expected size
        let expected_length = NETFLOW_V5_HEADER_SIZE + (count as usize * NETFLOW_V5_RECORD_SIZE);
        if data.len() < expected_length {
            return Err(format!(
                "Packet too short for {} flow records: {} bytes, need {}",
                count,
                data.len(),
                expected_length
            ));
        }

        Ok(Self {
            version,
            count,
            sys_uptime: u32::from_be_bytes([data[4], data[5], data[6], data[7]]),
            unix_secs: u32::from_be_bytes([data[8], data[9], data[10], data[11]]),
            unix_nsecs: u32::from_be_bytes([data[12], data[13], data[14], data[15]]),
            flow_sequence: u32::from_be_bytes([data[16], data[17], data[18], data[19]]),
            engine_type: data[20],
            engine_id: data[21],
            sampling_interval: u16::from_be_bytes([data[22], data[23]]),
        })
    }

    /// Header fields only (e.g. zero-flow datagrams or parse failure fallback).
    pub fn to_log_event(&self) -> LogEvent {
        let mut log_event = LogEvent::default();
        log_event.insert("version", self.version);
        log_event.insert("count", self.count);
        log_event.insert("sys_uptime", self.sys_uptime);
        log_event.insert("unix_secs", self.unix_secs);
        log_event.insert("unix_nsecs", self.unix_nsecs);
        log_event.insert("flow_sequence", self.flow_sequence);
        log_event.insert("engine_type", self.engine_type);
        log_event.insert("engine_id", self.engine_id);
        log_event.insert("sampling_interval", self.sampling_interval);
        log_event
    }
}

/// Parsed NetFlow v5 flow record (48-byte wire layout).
#[derive(Debug, Clone)]
pub struct NetflowV5Record {
    pub src_addr: u32,
    pub dst_addr: u32,
    pub next_hop: u32,
    pub input: u16,
    pub output: u16,
    pub d_pkts: u32,
    pub d_octets: u32,
    pub first: u32,
    pub last: u32,
    pub src_port: u16,
    pub dst_port: u16,
    pub pad1: u8,
    pub tcp_flags: u8,
    pub prot: u8,
    pub tos: u8,
    pub src_as: u16,
    pub dst_as: u16,
    pub src_mask: u8,
    pub dst_mask: u8,
    pub pad2: u16,
}

impl NetflowV5Record {
    /// Builds a record from exactly one 48-byte slice.
    pub fn from_bytes(data: &[u8]) -> Result<Self, String> {
        if data.len() < NETFLOW_V5_RECORD_SIZE {
            return Err(format!(
                "Insufficient data for NetFlow v5 record: {} bytes, need {}",
                data.len(),
                NETFLOW_V5_RECORD_SIZE
            ));
        }

        Ok(Self {
            src_addr: u32::from_be_bytes([data[0], data[1], data[2], data[3]]),
            dst_addr: u32::from_be_bytes([data[4], data[5], data[6], data[7]]),
            next_hop: u32::from_be_bytes([data[8], data[9], data[10], data[11]]),
            input: u16::from_be_bytes([data[12], data[13]]),
            output: u16::from_be_bytes([data[14], data[15]]),
            d_pkts: u32::from_be_bytes([data[16], data[17], data[18], data[19]]),
            d_octets: u32::from_be_bytes([data[20], data[21], data[22], data[23]]),
            first: u32::from_be_bytes([data[24], data[25], data[26], data[27]]),
            last: u32::from_be_bytes([data[28], data[29], data[30], data[31]]),
            src_port: u16::from_be_bytes([data[32], data[33]]),
            dst_port: u16::from_be_bytes([data[34], data[35]]),
            pad1: data[36],
            tcp_flags: data[37],
            prot: data[38],
            tos: data[39],
            src_as: u16::from_be_bytes([data[40], data[41]]),
            dst_as: u16::from_be_bytes([data[42], data[43]]),
            src_mask: data[44],
            dst_mask: data[45],
            pad2: u16::from_be_bytes([data[46], data[47]]),
        })
    }

    /// Formats a NetFlow v5 IPv4 field as dotted decimal (`u32` is big-endian / wire order).
    fn ipv4_to_string(addr: u32) -> String {
        Ipv4Addr::from(addr).to_string()
    }

    /// Cisco NetFlow v5 export field names (single pass: record + datagram header).
    pub fn to_log_event(&self, header: &NetflowV5Header) -> LogEvent {
        let mut log_event = LogEvent::default();

        log_event.insert("srcaddr", Self::ipv4_to_string(self.src_addr));
        log_event.insert("dstaddr", Self::ipv4_to_string(self.dst_addr));
        log_event.insert("nexthop", Self::ipv4_to_string(self.next_hop));
        log_event.insert("input", self.input);
        log_event.insert("output", self.output);
        log_event.insert("dpkts", self.d_pkts);
        log_event.insert("doctets", self.d_octets);
        log_event.insert("first", self.first);
        log_event.insert("last", self.last);
        log_event.insert("srcport", self.src_port);
        log_event.insert("dstport", self.dst_port);
        log_event.insert("tcp_flags", self.tcp_flags);
        log_event.insert("prot", self.prot);
        log_event.insert("tos", self.tos);
        log_event.insert("src_as", self.src_as);
        log_event.insert("dst_as", self.dst_as);
        log_event.insert("src_mask", self.src_mask);
        log_event.insert("dst_mask", self.dst_mask);

        log_event.insert("count", header.count);
        log_event.insert("sys_uptime", header.sys_uptime);
        log_event.insert("unix_secs", header.unix_secs);
        log_event.insert("unix_nsecs", header.unix_nsecs);
        log_event.insert("flow_sequence", header.flow_sequence);
        log_event.insert("engine_type", header.engine_type);
        log_event.insert("engine_id", header.engine_id);
        log_event.insert("sampling_interval", header.sampling_interval);
        log_event.insert("version", header.version);

        log_event
    }

    /// Validates counters and sizes when strict mode is enabled upstream.
    pub fn validate(&self) -> Result<(), String> {
        // Check for obviously invalid data
        if self.d_pkts == 0 && self.d_octets > 0 {
            return Err("Invalid record: zero packets but non-zero octets".to_string());
        }
        if self.d_pkts > 0 && self.d_octets < self.d_pkts {
            return Err(
                "Invalid record: octets less than packet count (bytes per packet < 1)".to_string(),
            );
        }
        if self.d_octets > 0 && self.d_pkts > 0 {
            let bytes_per_packet = self.d_octets / self.d_pkts;
            if bytes_per_packet < 1 || bytes_per_packet > 65535 {
                return Err(format!("Invalid bytes per packet: {}", bytes_per_packet));
            }
        }

        // first_switched/last_switched are sysUptime milliseconds; when last < first it is often
        // 32-bit wraparound on long-uptime devices, not bad data. Accept the record and emit.
        // Skip timing rejection to avoid dropping valid flows from real exporters.

        Ok(())
    }
}

/// Stateful NetFlow v5 packet parser (sequence tracking per exporter IP).
pub struct NetflowV5Parser {
    strict_validation: bool,
    /// Latest `(flow_sequence, record_count)` per exporter IP for gap detection (keyed by IP only).
    last_sequence_by_peer: DashMap<IpAddr, (u32, u16)>,
}

impl NetflowV5Parser {
    /// Creates a parser. `_field_parser` is unused for NetFlow v5 but keeps the constructor stable.
    pub fn new(_field_parser: FieldParser, strict_validation: bool) -> Self {
        Self {
            strict_validation,
            last_sequence_by_peer: DashMap::new(),
        }
    }

    /// Returns true when `data` begins with version 5 and is long enough for the claimed records.
    pub fn can_parse(data: &[u8]) -> bool {
        if data.len() < 2 {
            return false;
        }
        let version = u16::from_be_bytes([data[0], data[1]]);
        Self::can_parse_with_version(data, version)
    }

    /// Like [`Self::can_parse`], but uses a caller-supplied version word (avoids re-reading bytes).
    pub(crate) fn can_parse_with_version(data: &[u8], version: u16) -> bool {
        if version != NETFLOW_V5_VERSION {
            return false;
        }
        if data.len() < NETFLOW_V5_HEADER_SIZE {
            return false;
        }
        let count = u16::from_be_bytes([data[2], data[3]]);
        if count > MAX_FLOW_COUNT {
            return false;
        }
        let expected_length = NETFLOW_V5_HEADER_SIZE + (count as usize * NETFLOW_V5_RECORD_SIZE);
        data.len() >= expected_length
    }

    /// Parses a single UDP payload into log events (one per flow record when possible).
    pub fn parse(
        &self,
        data: &[u8],
        peer_addr: SocketAddr,
        include_raw_data: bool,
    ) -> Result<Vec<Event>, String> {
        // Parse header
        let header = NetflowV5Header::from_bytes(data)?;
        let mut events = Vec::with_capacity(header.count as usize);

        debug!(
            message = "Parsing NetFlow v5 packet.",
            version = header.version,
            count = header.count,
            sequence = header.flow_sequence,
        );

        let peer_ip = peer_addr.ip();
        if let Some(pair) = self.last_sequence_by_peer.get(&peer_ip) {
            let (last_seq, last_count) = *pair;
            let expected = last_seq.wrapping_add(last_count as u32);
            if expected != header.flow_sequence {
                debug!(
                    message = "Flow sequence gap detected.",
                    peer_addr = %peer_addr,
                    expected = expected,
                    received = header.flow_sequence,
                );
            }
        }
        self.last_sequence_by_peer
            .insert(peer_ip, (header.flow_sequence, header.count));

        let raw_data_encoded = include_raw_data
            .then(|| base64::Engine::encode(&base64::engine::general_purpose::STANDARD, data));

        // Parse flow records
        let mut record_offset = NETFLOW_V5_HEADER_SIZE;
        let mut valid_records = 0;
        let mut invalid_records = 0;
        let mut raw_data_attached = false;

        for i in 0..header.count {
            let record_end = record_offset + NETFLOW_V5_RECORD_SIZE;
            if record_end > data.len() {
                warn!(
                    message = "Insufficient data for record.",
                    record_index = i,
                    offset = record_offset,
                    need = NETFLOW_V5_RECORD_SIZE,
                );
                break;
            }

            let record_data = &data[record_offset..record_end];
            match NetflowV5Record::from_bytes(record_data) {
                Ok(record) => {
                    if self.strict_validation {
                        if let Err(validation_error) = record.validate() {
                            debug!(
                                message = "Invalid NetFlow v5 record.",
                                record_index = i,
                                error = %validation_error,
                            );
                            invalid_records += 1;
                            record_offset = record_end;
                            continue;
                        }
                    } else if let Err(validation_error) = record.validate() {
                        debug!(
                            message = "NetFlow v5 record validation warning.",
                            record_index = i,
                            error = %validation_error,
                        );
                    }

                    let mut record_event = record.to_log_event(&header);

                    if let Some(ref encoded) = raw_data_encoded {
                        if !raw_data_attached {
                            record_event.insert("raw_data", encoded.clone());
                            raw_data_attached = true;
                        }
                    }

                    events.push(Event::Log(record_event));
                    valid_records += 1;
                }
                Err(e) => {
                    warn!(
                        message = "Failed to parse NetFlow v5 record.",
                        record_index = i,
                        error = %e,
                    );
                    invalid_records += 1;
                }
            }

            record_offset = record_end;
        }

        if events.is_empty() {
            let mut base_event = header.to_log_event();
            if let Some(ref encoded) = raw_data_encoded {
                base_event.insert("raw_data", encoded.clone());
            }
            events.push(Event::Log(base_event));
        }

        debug!(
            message = "NetFlow v5 packet processed.",
            peer_addr = %peer_addr,
            total_records = header.count as usize,
            valid_records = valid_records,
            invalid_records = invalid_records,
            event_count = events.len(),
        );
        if invalid_records > 0 {
            warn!(
                message = "NetFlow v5 packet contained invalid records.",
                peer_addr = %peer_addr,
                invalid_records = invalid_records,
                total_records = header.count as usize,
            );
        }

        Ok(events)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sources::netflow::config::NetflowConfig;
    use crate::sources::netflow::fields::FieldParser;
    use std::net::{IpAddr, Ipv4Addr};

    fn test_peer_addr() -> SocketAddr {
        SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 100)), 2055)
    }

    fn create_netflow_v5_packet(record_count: u16) -> Vec<u8> {
        let mut packet =
            vec![0u8; NETFLOW_V5_HEADER_SIZE + (record_count as usize * NETFLOW_V5_RECORD_SIZE)];

        // Header
        packet[0..2].copy_from_slice(&5u16.to_be_bytes()); // version
        packet[2..4].copy_from_slice(&record_count.to_be_bytes()); // count
        packet[4..8].copy_from_slice(&12345u32.to_be_bytes()); // sys_uptime
        packet[8..12].copy_from_slice(&1609459200u32.to_be_bytes()); // unix_secs
        packet[12..16].copy_from_slice(&0u32.to_be_bytes()); // unix_nsecs
        packet[16..20].copy_from_slice(&100u32.to_be_bytes()); // flow_sequence
        packet[20] = 0; // engine_type
        packet[21] = 0; // engine_id
        packet[22..24].copy_from_slice(&0u16.to_be_bytes()); // sampling_interval

        // Add sample records
        for i in 0..record_count {
            let record_offset = NETFLOW_V5_HEADER_SIZE + (i as usize * NETFLOW_V5_RECORD_SIZE);

            // Sample flow record
            packet[record_offset..record_offset + 4].copy_from_slice(&0xC0A80101u32.to_be_bytes()); // src: 192.168.1.1
            packet[record_offset + 4..record_offset + 8]
                .copy_from_slice(&0x0A000001u32.to_be_bytes()); // dst: 10.0.0.1
            packet[record_offset + 16..record_offset + 20].copy_from_slice(&10u32.to_be_bytes()); // packets
            packet[record_offset + 20..record_offset + 24].copy_from_slice(&1500u32.to_be_bytes()); // octets
            packet[record_offset + 32..record_offset + 34].copy_from_slice(&80u16.to_be_bytes()); // src_port
            packet[record_offset + 34..record_offset + 36].copy_from_slice(&443u16.to_be_bytes()); // dst_port
            packet[record_offset + 38] = 6; // protocol (TCP)
        }

        packet
    }

    #[test]
    fn test_netflow_v5_header_parsing() {
        let packet = create_netflow_v5_packet(1);
        let header = NetflowV5Header::from_bytes(&packet).unwrap();

        assert_eq!(header.version, 5);
        assert_eq!(header.count, 1);
        assert_eq!(header.sys_uptime, 12345);
        assert_eq!(header.unix_secs, 1609459200);
        assert_eq!(header.flow_sequence, 100);
        assert_eq!(header.engine_type, 0);
        assert_eq!(header.engine_id, 0);
        assert_eq!(header.sampling_interval, 0);
    }

    #[test]
    fn test_invalid_netflow_v5_header() {
        // Too short
        let short_packet = vec![0u8; 10];
        assert!(NetflowV5Header::from_bytes(&short_packet).is_err());

        // Wrong version
        let mut wrong_version = create_netflow_v5_packet(1);
        wrong_version[0..2].copy_from_slice(&9u16.to_be_bytes());
        assert!(NetflowV5Header::from_bytes(&wrong_version).is_err());

        // Too many flows
        let mut too_many = create_netflow_v5_packet(1);
        too_many[2..4].copy_from_slice(&2000u16.to_be_bytes());
        assert!(NetflowV5Header::from_bytes(&too_many).is_err());

        // Packet too short for claimed record count
        let mut short_for_records = vec![0u8; NETFLOW_V5_HEADER_SIZE + 10];
        short_for_records[0..2].copy_from_slice(&5u16.to_be_bytes()); // version
        short_for_records[2..4].copy_from_slice(&2u16.to_be_bytes()); // count = 2
        assert!(NetflowV5Header::from_bytes(&short_for_records).is_err());
    }

    #[test]
    fn test_can_parse() {
        // Valid NetFlow v5
        let packet = create_netflow_v5_packet(1);
        assert!(NetflowV5Parser::can_parse(&packet));

        // Invalid version
        let mut invalid_version = packet.clone();
        invalid_version[0..2].copy_from_slice(&10u16.to_be_bytes());
        assert!(!NetflowV5Parser::can_parse(&invalid_version));

        // Zero count is valid (header-only packet)
        let mut zero_count = packet.clone();
        zero_count[2..4].copy_from_slice(&0u16.to_be_bytes());
        assert!(NetflowV5Parser::can_parse(&zero_count));

        // Too short
        let short_packet = vec![0u8; 10];
        assert!(!NetflowV5Parser::can_parse(&short_packet));
    }

    #[test]
    fn test_record_parsing() {
        let record_data = vec![
            192, 168, 1, 1, // src_addr
            10, 0, 0, 1, // dst_addr
            0, 0, 0, 0, // next_hop
            0, 1, // input
            0, 2, // output
            0, 0, 0, 10, // d_pkts
            0, 0, 5, 220, // d_octets (1500)
            0, 0, 0, 100, // first
            0, 0, 0, 200, // last
            0, 80, // src_port
            1, 187, // dst_port (443)
            0,   // pad1
            24,  // tcp_flags (ACK+PSH)
            6,   // protocol (TCP)
            0,   // tos
            0, 100, // src_as
            0, 200, // dst_as
            24,  // src_mask
            8,   // dst_mask
            0, 0, // pad2
        ];

        let record = NetflowV5Record::from_bytes(&record_data).unwrap();

        assert_eq!(record.src_addr, 0xC0A80101); // 192.168.1.1
        assert_eq!(record.dst_addr, 0x0A000001); // 10.0.0.1
        assert_eq!(record.src_port, 80);
        assert_eq!(record.dst_port, 443);
        assert_eq!(record.prot, 6);
        assert_eq!(record.d_pkts, 10);
        assert_eq!(record.d_octets, 1500);
        assert_eq!(record.tcp_flags, 24);
    }

    #[test]
    fn test_record_to_log_event() {
        let packet = create_netflow_v5_packet(1);
        let header = NetflowV5Header::from_bytes(&packet).unwrap();
        let record = NetflowV5Record::from_bytes(&packet[NETFLOW_V5_HEADER_SIZE..]).unwrap();
        let log_event = record.to_log_event(&header);

        assert_eq!(
            log_event.get("srcaddr").unwrap().as_str().unwrap(),
            "192.168.1.1"
        );
        assert_eq!(
            log_event.get("dstaddr").unwrap().as_str().unwrap(),
            "10.0.0.1"
        );
        assert_eq!(log_event.get("srcport").unwrap().as_integer().unwrap(), 80);
        assert_eq!(
            log_event.get("dstport").unwrap().as_integer().unwrap(),
            443
        );
        assert_eq!(log_event.get("prot").unwrap().as_integer().unwrap(), 6);
        assert_eq!(log_event.get("dpkts").unwrap().as_integer().unwrap(), 10);
        assert_eq!(log_event.get("doctets").unwrap().as_integer().unwrap(), 1500);
        assert_eq!(log_event.get("tcp_flags").unwrap().as_integer().unwrap(), 0);
        assert_eq!(
            log_event.get("flow_sequence").unwrap().as_integer().unwrap(),
            100
        );
        assert_eq!(log_event.get("count").unwrap().as_integer().unwrap(), 1);
        assert_eq!(log_event.get("version").unwrap().as_integer().unwrap(), 5);
    }

    #[test]
    fn test_record_validation() {
        let mut record_data = vec![0u8; NETFLOW_V5_RECORD_SIZE];

        // Valid record
        record_data[16..20].copy_from_slice(&10u32.to_be_bytes()); // packets
        record_data[20..24].copy_from_slice(&1500u32.to_be_bytes()); // octets
        record_data[24..28].copy_from_slice(&100u32.to_be_bytes()); // first
        record_data[28..32].copy_from_slice(&200u32.to_be_bytes()); // last
        let record = NetflowV5Record::from_bytes(&record_data).unwrap();
        assert!(record.validate().is_ok());

        // Invalid: zero packets but non-zero octets
        record_data[16..20].copy_from_slice(&0u32.to_be_bytes()); // packets = 0
        record_data[20..24].copy_from_slice(&1500u32.to_be_bytes()); // octets = 1500
        let record = NetflowV5Record::from_bytes(&record_data).unwrap();
        assert!(record.validate().is_err());

        // first/last (sysUptime ms) are not rejected when last < first; wraparound is accepted
        record_data[16..20].copy_from_slice(&10u32.to_be_bytes()); // packets
        record_data[20..24].copy_from_slice(&1500u32.to_be_bytes()); // octets
        record_data[24..28].copy_from_slice(&3_601_000u32.to_be_bytes()); // first
        record_data[28..32].copy_from_slice(&100u32.to_be_bytes()); // last
        let record = NetflowV5Record::from_bytes(&record_data).unwrap();
        assert!(record.validate().is_ok());
    }

    #[test]
    fn test_full_packet_parsing() {
        let config = NetflowConfig::default();
        let field_parser = FieldParser::new(&config);
        let parser = NetflowV5Parser::new(field_parser, true);

        let packet = create_netflow_v5_packet(2);
        let events = parser.parse(&packet, test_peer_addr(), false).unwrap();

        // Should get 2 record events
        assert_eq!(events.len(), 2);

        for event in &events {
            if let Event::Log(log) = event {
                assert_eq!(
                    log.get("srcaddr").unwrap().as_str().unwrap(),
                    "192.168.1.1"
                );
                assert_eq!(log.get("dstaddr").unwrap().as_str().unwrap(), "10.0.0.1");
                assert_eq!(
                    log.get("flow_sequence").unwrap().as_integer().unwrap(),
                    100
                );
            }
        }
    }

    #[test]
    fn test_parsing_with_raw_data() {
        let config = NetflowConfig::default();
        let field_parser = FieldParser::new(&config);
        let parser = NetflowV5Parser::new(field_parser, true);

        let packet = create_netflow_v5_packet(1);

        // Test with raw data inclusion
        let events = parser.parse(&packet, test_peer_addr(), true).unwrap();
        assert!(!events.is_empty());

        assert!(events[0].as_log().get("raw_data").is_some());
    }

    #[test]
    fn test_zero_records_packet() {
        let config = NetflowConfig::default();
        let field_parser = FieldParser::new(&config);
        let parser = NetflowV5Parser::new(field_parser, true);

        let packet = create_netflow_v5_packet(0);
        let events = parser.parse(&packet, test_peer_addr(), false).unwrap();

        // Should get header event only
        assert_eq!(events.len(), 1);

        if let Event::Log(log) = &events[0] {
            assert_eq!(log.get("count").unwrap().as_integer().unwrap(), 0);
            assert_eq!(log.get("version").unwrap().as_integer().unwrap(), 5);
        }
    }

    #[test]
    fn test_malformed_record_handling() {
        let config = NetflowConfig::default();
        let field_parser = FieldParser::new(&config);
        let parser = NetflowV5Parser::new(field_parser, true);

        // Create packet with malformed record (invalid packet count)
        let mut packet = create_netflow_v5_packet(1);
        // Make the record invalid by setting packets=0, octets=1000
        let record_offset = NETFLOW_V5_HEADER_SIZE;
        packet[record_offset + 16..record_offset + 20].copy_from_slice(&0u32.to_be_bytes()); // packets = 0
        packet[record_offset + 20..record_offset + 24].copy_from_slice(&1000u32.to_be_bytes()); // octets = 1000

        let events = parser.parse(&packet, test_peer_addr(), false).unwrap();

        // Should handle gracefully, likely with header event
        assert!(!events.is_empty());
    }

    #[test]
    fn test_invalid_bytes_per_packet() {
        let mut record_data = vec![0u8; NETFLOW_V5_RECORD_SIZE];

        // Invalid: 1 packet, 0 bytes (below minimum 1 byte per packet)
        record_data[16..20].copy_from_slice(&1u32.to_be_bytes()); // d_pkts = 1
        record_data[20..24].copy_from_slice(&0u32.to_be_bytes()); // d_octets = 0
        let record = NetflowV5Record::from_bytes(&record_data).unwrap();
        assert!(record.validate().is_err());

        // Invalid: 1 packet, 100000 bytes (over 65535)
        record_data[20..24].copy_from_slice(&100000u32.to_be_bytes());
        let record = NetflowV5Record::from_bytes(&record_data).unwrap();
        assert!(record.validate().is_err());
    }

    #[test]
    fn test_multiple_records_with_validation() {
        let config = NetflowConfig::default();
        let field_parser = FieldParser::new(&config);
        let parser = NetflowV5Parser::new(field_parser, true);

        // Create packet with mix of valid and invalid records
        let mut packet = create_netflow_v5_packet(3);

        // Make second record invalid (zero packets, non-zero octets)
        let second_record_offset = NETFLOW_V5_HEADER_SIZE + NETFLOW_V5_RECORD_SIZE;
        packet[second_record_offset + 16..second_record_offset + 20]
            .copy_from_slice(&0u32.to_be_bytes()); // packets = 0
        packet[second_record_offset + 20..second_record_offset + 24]
            .copy_from_slice(&1500u32.to_be_bytes()); // octets = 1500

        let events = parser.parse(&packet, test_peer_addr(), false).unwrap();

        // Should get 2 valid records (first and third)
        assert_eq!(events.len(), 2);

        if let Event::Log(log) = &events[0] {
            assert_eq!(
                log.get("flow_sequence").unwrap().as_integer().unwrap(),
                100
            );
        }
        if let Event::Log(log) = &events[1] {
            assert_eq!(
                log.get("flow_sequence").unwrap().as_integer().unwrap(),
                100
            );
        }
    }

    #[test]
    fn test_header_to_log_event() {
        let packet = create_netflow_v5_packet(1);
        let header = NetflowV5Header::from_bytes(&packet).unwrap();
        let log_event = header.to_log_event();

        assert_eq!(log_event.get("version").unwrap().as_integer().unwrap(), 5);
        assert_eq!(log_event.get("count").unwrap().as_integer().unwrap(), 1);
        assert_eq!(
            log_event.get("sys_uptime").unwrap().as_integer().unwrap(),
            12345
        );
        assert_eq!(
            log_event.get("unix_secs").unwrap().as_integer().unwrap(),
            1609459200
        );
        assert_eq!(
            log_event
                .get("flow_sequence")
                .unwrap()
                .as_integer()
                .unwrap(),
            100
        );
        assert_eq!(
            log_event.get("engine_type").unwrap().as_integer().unwrap(),
            0
        );
        assert_eq!(log_event.get("engine_id").unwrap().as_integer().unwrap(), 0);
        assert_eq!(
            log_event
                .get("sampling_interval")
                .unwrap()
                .as_integer()
                .unwrap(),
            0
        );
    }

    #[test]
    fn test_ipv4_address_conversion() {
        assert_eq!(NetflowV5Record::ipv4_to_string(0xC0A80101), "192.168.1.1");
        assert_eq!(NetflowV5Record::ipv4_to_string(0x08080808), "8.8.8.8");
        assert_eq!(NetflowV5Record::ipv4_to_string(0x00000000), "0.0.0.0");
        assert_eq!(
            NetflowV5Record::ipv4_to_string(0xFFFFFFFF),
            "255.255.255.255"
        );
    }
}
