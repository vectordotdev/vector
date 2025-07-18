//! NetFlow v5 protocol parser.
//!
//! NetFlow v5 is a fixed-format flow export protocol that defines a standard
//! structure for flow records. Unlike template-based protocols (v9/IPFIX),
//! NetFlow v5 has a rigid 48-byte record format that contains predefined fields.

use crate::sources::netflow::events::*;
use crate::sources::netflow::fields::FieldParser;

use std::net::SocketAddr;
use tracing::{debug, warn};
use vector_lib::event::{Event, LogEvent};


/// NetFlow v5 protocol constants
const NETFLOW_V5_VERSION: u16 = 5;
const NETFLOW_V5_HEADER_SIZE: usize = 24;
const NETFLOW_V5_RECORD_SIZE: usize = 48;
const MAX_FLOW_COUNT: u16 = 1000; // Sanity check for flow count

/// NetFlow v5 packet header structure
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
    /// Parse NetFlow v5 header from packet data
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
                count, data.len(), expected_length
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

    /// Create base log event with header information
    pub fn to_log_event(&self) -> LogEvent {
        let mut log_event = LogEvent::default();
        log_event.insert("flow_type", "netflow_v5");
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

    /// Get sampling rate from sampling interval
    pub fn sampling_rate(&self) -> u32 {
        if self.sampling_interval == 0 {
            1 // No sampling
        } else {
            // Upper 14 bits are the sampling mode, lower 14 bits are the interval
            let interval = self.sampling_interval & 0x3FFF;
            if interval == 0 { 1 } else { interval as u32 }
        }
    }

    /// Check if sampling is enabled
    pub fn is_sampled(&self) -> bool {
        self.sampling_interval != 0
    }
}

/// NetFlow v5 flow record structure
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
    /// Parse NetFlow v5 record from packet data
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

    /// Convert IPv4 address to string
    fn ipv4_to_string(addr: u32) -> String {
        format!(
            "{}.{}.{}.{}",
            (addr >> 24) & 0xFF,
            (addr >> 16) & 0xFF,
            (addr >> 8) & 0xFF,
            addr & 0xFF
        )
    }

    /// Get protocol name from protocol number
    fn get_protocol_name(protocol: u8) -> &'static str {
        match protocol {
            1 => "ICMP",
            6 => "TCP",
            17 => "UDP",
            47 => "GRE",
            50 => "ESP",
            51 => "AH",
            89 => "OSPF",
            132 => "SCTP",
            _ => "Other",
        }
    }

    /// Convert record to log event
    pub fn to_log_event(&self, record_number: usize, resolve_protocols: bool) -> LogEvent {
        let mut log_event = LogEvent::default();
        
        // Flow identification
        log_event.insert("flow_type", "netflow_v5_record");
        log_event.insert("record_number", record_number);
        
        // Network addresses
        log_event.insert("src_addr", Self::ipv4_to_string(self.src_addr));
        log_event.insert("dst_addr", Self::ipv4_to_string(self.dst_addr));
        log_event.insert("next_hop", Self::ipv4_to_string(self.next_hop));
        
        // Interface information
        log_event.insert("input_interface", self.input);
        log_event.insert("output_interface", self.output);
        
        // Traffic counters
        log_event.insert("packets", self.d_pkts);
        log_event.insert("octets", self.d_octets);
        
        // Timing information
        log_event.insert("first_switched", self.first);
        log_event.insert("last_switched", self.last);
        
        // Port information
        log_event.insert("src_port", self.src_port);
        log_event.insert("dst_port", self.dst_port);
        
        // Protocol information
        log_event.insert("protocol", self.prot);
        if resolve_protocols {
            log_event.insert("protocol_name", Self::get_protocol_name(self.prot));
        }
        
        // TCP flags
        log_event.insert("tcp_flags", self.tcp_flags);
        if self.prot == 6 { // TCP
            log_event.insert("tcp_flags_urg", (self.tcp_flags & 0x20) != 0);
            log_event.insert("tcp_flags_ack", (self.tcp_flags & 0x10) != 0);
            log_event.insert("tcp_flags_psh", (self.tcp_flags & 0x08) != 0);
            log_event.insert("tcp_flags_rst", (self.tcp_flags & 0x04) != 0);
            log_event.insert("tcp_flags_syn", (self.tcp_flags & 0x02) != 0);
            log_event.insert("tcp_flags_fin", (self.tcp_flags & 0x01) != 0);
        }
        
        // Type of Service
        log_event.insert("tos", self.tos);
        log_event.insert("dscp", (self.tos >> 2) & 0x3F); // DSCP is upper 6 bits
        log_event.insert("ecn", self.tos & 0x03); // ECN is lower 2 bits
        
        // AS information
        log_event.insert("src_as", self.src_as);
        log_event.insert("dst_as", self.dst_as);
        
        // Subnet mask information
        log_event.insert("src_mask", self.src_mask);
        log_event.insert("dst_mask", self.dst_mask);
        
        // Calculate flow duration if possible
        if self.last > self.first {
            log_event.insert("flow_duration_ms", self.last - self.first);
        }
        
        // Calculate bytes per packet
        if self.d_pkts > 0 {
            log_event.insert("bytes_per_packet", self.d_octets / self.d_pkts);
        }
        
        // Flow direction heuristics
        let flow_direction = self.determine_flow_direction();
        log_event.insert("flow_direction", flow_direction);
        
        log_event
    }

    /// Determine flow direction based on port analysis
    fn determine_flow_direction(&self) -> &'static str {
        // Common server ports
        let server_ports = [
            21, 22, 23, 25, 53, 80, 110, 143, 443, 993, 995, 
            1433, 3306, 5432, 6379, 27017
        ];
        
        let src_is_server = server_ports.contains(&self.src_port);
        let dst_is_server = server_ports.contains(&self.dst_port);
        
        match (src_is_server, dst_is_server) {
            (true, false) => "outbound", // Server to client
            (false, true) => "inbound",  // Client to server
            _ => {
                // Use port number heuristic
                if self.src_port < self.dst_port {
                    "outbound"
                } else if self.dst_port < self.src_port {
                    "inbound"
                } else {
                    "unknown"
                }
            }
        }
    }

    /// Validate record data for reasonableness
    pub fn validate(&self) -> Result<(), String> {
        // Check for obviously invalid data
        if self.d_pkts == 0 && self.d_octets > 0 {
            return Err("Invalid record: zero packets but non-zero octets".to_string());
        }
        
        if self.d_octets > 0 && self.d_pkts > 0 {
            let bytes_per_packet = self.d_octets / self.d_pkts;
            if bytes_per_packet < 20 || bytes_per_packet > 65535 {
                return Err(format!("Invalid bytes per packet: {}", bytes_per_packet));
            }
        }
        
        // Check timing
        if self.last < self.first {
            return Err("Invalid timing: last < first".to_string());
        }
        
        // Check for private/reserved addresses in next_hop if it's not zero
        if self.next_hop != 0 {
            let is_private = self.is_private_ipv4(self.next_hop);
            let is_zero = self.next_hop == 0;
            let is_broadcast = self.next_hop == 0xFFFFFFFF;
            
            if !is_private && !is_zero && !is_broadcast {
                // Public next hop - this is normal for routed traffic
            }
        }
        
        Ok(())
    }
    
    /// Check if IPv4 address is in private range
    fn is_private_ipv4(&self, addr: u32) -> bool {
        let a = (addr >> 24) & 0xFF;
        let b = (addr >> 16) & 0xFF;
        
        // 10.0.0.0/8, 172.16.0.0/12, 192.168.0.0/16
        a == 10 || (a == 172 && b >= 16 && b <= 31) || (a == 192 && b == 168)
    }
}

/// NetFlow v5 protocol parser
pub struct NetflowV5Parser {
}

impl NetflowV5Parser {
    /// Create a new NetFlow v5 parser
    pub fn new(_field_parser: FieldParser) -> Self {
        Self {}
    }

    /// Check if packet data looks like NetFlow v5
    pub fn can_parse(data: &[u8]) -> bool {
        if data.len() < NETFLOW_V5_HEADER_SIZE {
            return false;
        }

        let version = u16::from_be_bytes([data[0], data[1]]);
        if version != NETFLOW_V5_VERSION {
            return false;
        }

        let count = u16::from_be_bytes([data[2], data[3]]);
        if count == 0 || count > MAX_FLOW_COUNT {
            return false;
        }

        // Check if packet length is reasonable
        let expected_length = NETFLOW_V5_HEADER_SIZE + (count as usize * NETFLOW_V5_RECORD_SIZE);
        if data.len() < expected_length {
            return false;
        }

        true
    }

    /// Parse NetFlow v5 packet and return events
    pub fn parse(
        &self,
        data: &[u8],
        peer_addr: SocketAddr,
        include_raw_data: bool,
    ) -> Result<Vec<Event>, String> {
        let mut events = Vec::new();

        // Parse header
        let header = NetflowV5Header::from_bytes(data)?;
        
        debug!(
            "Parsing NetFlow v5 packet: version={}, count={}, sequence={}",
            header.version, header.count, header.flow_sequence
        );

        // Create base event with header info
        let mut base_event = header.to_log_event();
        base_event.insert("sampling_rate", header.sampling_rate());
        base_event.insert("is_sampled", header.is_sampled());
        
        if include_raw_data {
            let encoded = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, data);
            base_event.insert("raw_data", encoded);
        }

        // Parse flow records
        let mut record_offset = NETFLOW_V5_HEADER_SIZE;
        let mut valid_records = 0;
        let mut invalid_records = 0;

        for i in 0..header.count {
            let record_end = record_offset + NETFLOW_V5_RECORD_SIZE;
            if record_end > data.len() {
                warn!(
                    "Insufficient data for record {}: offset={}, need={}",
                    i, record_offset, NETFLOW_V5_RECORD_SIZE
                );
                break;
            }

            let record_data = &data[record_offset..record_end];
            match NetflowV5Record::from_bytes(record_data) {
                Ok(record) => {
                    // Validate record
                    if let Err(validation_error) = record.validate() {
                        warn!(
                            "Invalid NetFlow v5 record {}: {}",
                            i, validation_error
                        );
                        invalid_records += 1;
                        record_offset = record_end;
                        continue;
                    }

                    // Convert to log event
                    let mut record_event = record.to_log_event(i as usize, true); // resolve_protocols = true
                    
                    // Add header context to each record
                    record_event.insert("packet_sequence", header.flow_sequence);
                    record_event.insert("engine_type", header.engine_type);
                    record_event.insert("engine_id", header.engine_id);
                    record_event.insert("sys_uptime", header.sys_uptime);
                    record_event.insert("unix_secs", header.unix_secs);
                    record_event.insert("sampling_rate", header.sampling_rate());
                    
                    // Add raw data if requested
                    if include_raw_data {
                        let encoded = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, data);
                        record_event.insert("raw_data", encoded);
                    }
                    
                    events.push(Event::Log(record_event));
                    valid_records += 1;

                    emit!(DataRecordParsed {
                        template_id: 0, // NetFlow v5 doesn't use templates
                        fields_parsed: 20, // Approximate field count
                        record_size: NETFLOW_V5_RECORD_SIZE,
                        peer_addr,
                        protocol: "netflow_v5",
                    });
                }
                Err(e) => {
                    warn!("Failed to parse NetFlow v5 record {}: {}", i, e);
                    invalid_records += 1;
                }
            }

            record_offset = record_end;
        }

        // If no valid records were parsed, include header event
        if events.is_empty() {
            base_event.insert("valid_records", valid_records);
            base_event.insert("invalid_records", invalid_records);
            events.push(Event::Log(base_event));
        }

        emit!(NetflowV5PacketProcessed {
            peer_addr,
            total_records: header.count as usize,
            valid_records,
            invalid_records,
            event_count: events.len(),
        });

        Ok(events)
    }
}

/// NetFlow v5 specific events
#[derive(Debug)]
pub struct NetflowV5PacketProcessed {
    pub peer_addr: SocketAddr,
    pub total_records: usize,
    pub valid_records: usize,
    pub invalid_records: usize,
    pub event_count: usize,
}

impl vector_lib::internal_event::InternalEvent for NetflowV5PacketProcessed {
    fn emit(self) {
        debug!(
            message = "NetFlow v5 packet processed",
            peer_addr = %self.peer_addr,
            total_records = self.total_records,
            valid_records = self.valid_records,
            invalid_records = self.invalid_records,
            event_count = self.event_count,
        );
        
        if self.invalid_records > 0 {
            warn!(
                message = "NetFlow v5 packet contained invalid records",
                peer_addr = %self.peer_addr,
                invalid_records = self.invalid_records,
                total_records = self.total_records,
            );
        }
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
        let mut packet = vec![0u8; NETFLOW_V5_HEADER_SIZE + (record_count as usize * NETFLOW_V5_RECORD_SIZE)];
        
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
            packet[record_offset + 4..record_offset + 8].copy_from_slice(&0x0A000001u32.to_be_bytes()); // dst: 10.0.0.1
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

        // Zero count
        let mut zero_count = packet.clone();
        zero_count[2..4].copy_from_slice(&0u16.to_be_bytes());
        assert!(!NetflowV5Parser::can_parse(&zero_count));

        // Too short
        let short_packet = vec![0u8; 10];
        assert!(!NetflowV5Parser::can_parse(&short_packet));
    }

    #[test]
    fn test_record_parsing() {
        let record_data = vec![
            192, 168, 1, 1,    // src_addr
            10, 0, 0, 1,       // dst_addr
            0, 0, 0, 0,        // next_hop
            0, 1,              // input
            0, 2,              // output
            0, 0, 0, 10,       // d_pkts
            0, 0, 5, 220,      // d_octets (1500)
            0, 0, 0, 100,      // first
            0, 0, 0, 200,      // last
            0, 80,             // src_port
            1, 187,            // dst_port (443)
            0,                 // pad1
            24,                // tcp_flags (ACK+PSH)
            6,                 // protocol (TCP)
            0,                 // tos
            0, 100,            // src_as
            0, 200,            // dst_as
            24,                // src_mask
            8,                 // dst_mask
            0, 0,              // pad2
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
        let record_data = vec![
            192, 168, 1, 1,    // src_addr
            10, 0, 0, 1,       // dst_addr
            0, 0, 0, 0,        // next_hop
            0, 1,              // input
            0, 2,              // output
            0, 0, 0, 10,       // d_pkts
            0, 0, 5, 220,      // d_octets (1500)
            0, 0, 0, 100,      // first
            0, 0, 0, 200,      // last
            0, 80,             // src_port
            1, 187,            // dst_port (443)
            0,                 // pad1
            24,                // tcp_flags (ACK+PSH)
            6,                 // protocol (TCP)
            0,                 // tos
            0, 100,            // src_as
            0, 200,            // dst_as
            24,                // src_mask
            8,                 // dst_mask
            0, 0,              // pad2
        ];

        let record = NetflowV5Record::from_bytes(&record_data).unwrap();
        let log_event = record.to_log_event(0, true);

        assert_eq!(log_event.get("flow_type").unwrap().as_str().unwrap(), "netflow_v5_record");
        assert_eq!(log_event.get("src_addr").unwrap().as_str().unwrap(), "192.168.1.1");
        assert_eq!(log_event.get("dst_addr").unwrap().as_str().unwrap(), "10.0.0.1");
       assert_eq!(log_event.get("src_port").unwrap().as_integer().unwrap(), 80);
       assert_eq!(log_event.get("dst_port").unwrap().as_integer().unwrap(), 443);
       assert_eq!(log_event.get("protocol").unwrap().as_integer().unwrap(), 6);
       assert_eq!(log_event.get("protocol_name").unwrap().as_str().unwrap(), "TCP");
       assert_eq!(log_event.get("packets").unwrap().as_integer().unwrap(), 10);
       assert_eq!(log_event.get("octets").unwrap().as_integer().unwrap(), 1500);
       assert_eq!(log_event.get("flow_duration_ms").unwrap().as_integer().unwrap(), 100);
       assert_eq!(log_event.get("bytes_per_packet").unwrap().as_integer().unwrap(), 150);
       
       // Check TCP flags
       assert_eq!(log_event.get("tcp_flags_ack").unwrap().as_boolean().unwrap(), true);
       assert_eq!(log_event.get("tcp_flags_psh").unwrap().as_boolean().unwrap(), true);
       assert_eq!(log_event.get("tcp_flags_syn").unwrap().as_boolean().unwrap(), false);
       
       // Check DSCP/ECN
       assert_eq!(log_event.get("dscp").unwrap().as_integer().unwrap(), 0);
       assert_eq!(log_event.get("ecn").unwrap().as_integer().unwrap(), 0);
   }

   #[test]
   fn test_sampling_rate_calculation() {
       let mut packet = create_netflow_v5_packet(1);
       
       // Test no sampling
       packet[22..24].copy_from_slice(&0u16.to_be_bytes());
       let header = NetflowV5Header::from_bytes(&packet).unwrap();
       assert_eq!(header.sampling_rate(), 1);
       assert!(!header.is_sampled());
       
       // Test 1:100 sampling
       packet[22..24].copy_from_slice(&100u16.to_be_bytes());
       let header = NetflowV5Header::from_bytes(&packet).unwrap();
       assert_eq!(header.sampling_rate(), 100);
       assert!(header.is_sampled());
   }

   #[test]
   fn test_flow_direction_detection() {
       let mut record_data = vec![0u8; NETFLOW_V5_RECORD_SIZE];
       
       // Client to server (high port to low port)
       record_data[32..34].copy_from_slice(&50000u16.to_be_bytes()); // src_port
       record_data[34..36].copy_from_slice(&80u16.to_be_bytes());    // dst_port (HTTP)
       let record = NetflowV5Record::from_bytes(&record_data).unwrap();
       assert_eq!(record.determine_flow_direction(), "inbound");
       
       // Server to client (low port to high port)
       record_data[32..34].copy_from_slice(&80u16.to_be_bytes());    // src_port (HTTP)
       record_data[34..36].copy_from_slice(&50000u16.to_be_bytes()); // dst_port
       let record = NetflowV5Record::from_bytes(&record_data).unwrap();
       assert_eq!(record.determine_flow_direction(), "outbound");
   }

   #[test]
   fn test_record_validation() {
       let mut record_data = vec![0u8; NETFLOW_V5_RECORD_SIZE];
       
       // Valid record
       record_data[16..20].copy_from_slice(&10u32.to_be_bytes());   // packets
       record_data[20..24].copy_from_slice(&1500u32.to_be_bytes()); // octets
       record_data[24..28].copy_from_slice(&100u32.to_be_bytes());  // first
       record_data[28..32].copy_from_slice(&200u32.to_be_bytes());  // last
       let record = NetflowV5Record::from_bytes(&record_data).unwrap();
       assert!(record.validate().is_ok());
       
       // Invalid: zero packets but non-zero octets
       record_data[16..20].copy_from_slice(&0u32.to_be_bytes());    // packets = 0
       record_data[20..24].copy_from_slice(&1500u32.to_be_bytes()); // octets = 1500
       let record = NetflowV5Record::from_bytes(&record_data).unwrap();
       assert!(record.validate().is_err());
       
       // Invalid: last < first
       record_data[16..20].copy_from_slice(&10u32.to_be_bytes());   // packets
       record_data[20..24].copy_from_slice(&1500u32.to_be_bytes()); // octets
       record_data[24..28].copy_from_slice(&200u32.to_be_bytes());  // first
       record_data[28..32].copy_from_slice(&100u32.to_be_bytes());  // last
       let record = NetflowV5Record::from_bytes(&record_data).unwrap();
       assert!(record.validate().is_err());
   }

   #[test]
   fn test_private_ip_detection() {
       let record = NetflowV5Record {
           src_addr: 0, dst_addr: 0, next_hop: 0, input: 0, output: 0,
           d_pkts: 0, d_octets: 0, first: 0, last: 0, src_port: 0,
           dst_port: 0, pad1: 0, tcp_flags: 0, prot: 0, tos: 0,
           src_as: 0, dst_as: 0, src_mask: 0, dst_mask: 0, pad2: 0,
       };
       
       // 10.0.0.1
       assert!(record.is_private_ipv4(0x0A000001));
       // 172.16.0.1
       assert!(record.is_private_ipv4(0xAC100001));
       // 192.168.1.1
       assert!(record.is_private_ipv4(0xC0A80101));
       // 8.8.8.8 (public)
       assert!(!record.is_private_ipv4(0x08080808));
   }

   #[test]
   fn test_full_packet_parsing() {
       let config = NetflowConfig::default();
       let field_parser = FieldParser::new(&config);
       let parser = NetflowV5Parser::new(field_parser);

       let packet = create_netflow_v5_packet(2);
       let events = parser.parse(&packet, test_peer_addr(), false).unwrap();

       // Should get 2 record events
       assert_eq!(events.len(), 2);
       
       for (i, event) in events.iter().enumerate() {
           if let Event::Log(log) = event {
               assert_eq!(log.get("flow_type").unwrap().as_str().unwrap(), "netflow_v5_record");
               assert_eq!(log.get("record_number").unwrap().as_integer().unwrap(), i as i64);
               assert_eq!(log.get("src_addr").unwrap().as_str().unwrap(), "192.168.1.1");
               assert_eq!(log.get("dst_addr").unwrap().as_str().unwrap(), "10.0.0.1");
               assert_eq!(log.get("packet_sequence").unwrap().as_integer().unwrap(), 100);
           }
       }
   }

   #[test]
   fn test_parsing_with_raw_data() {
       let config = NetflowConfig::default();
       let field_parser = FieldParser::new(&config);
       let parser = NetflowV5Parser::new(field_parser);

       let packet = create_netflow_v5_packet(1);
       
       // Test with raw data inclusion
       let events = parser.parse(&packet, test_peer_addr(), true).unwrap();
       assert!(!events.is_empty());
       
       // Raw data should be included in individual records when requested
       for event in &events {
           if let Event::Log(log) = event {
               let flow_type = log.get("flow_type").unwrap().as_str().unwrap();
               if flow_type == "netflow_v5_record" {
                   // Records should include raw data when requested
                   assert!(log.get("raw_data").is_some());
               }
           }
       }
   }

   #[test]
   fn test_zero_records_packet() {
       let config = NetflowConfig::default();
       let field_parser = FieldParser::new(&config);
       let parser = NetflowV5Parser::new(field_parser);

       let packet = create_netflow_v5_packet(0);
       let events = parser.parse(&packet, test_peer_addr(), false).unwrap();

       // Should get header event only
       assert_eq!(events.len(), 1);
       
       if let Event::Log(log) = &events[0] {
           assert_eq!(log.get("flow_type").unwrap().as_str().unwrap(), "netflow_v5");
           assert_eq!(log.get("count").unwrap().as_integer().unwrap(), 0);
       }
   }

   #[test]
   fn test_malformed_record_handling() {
       let config = NetflowConfig::default();
       let field_parser = FieldParser::new(&config);
       let parser = NetflowV5Parser::new(field_parser);

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
       
       // Invalid: 1 packet, 10 bytes (too small)
       record_data[16..20].copy_from_slice(&1u32.to_be_bytes());    // packets = 1
       record_data[20..24].copy_from_slice(&10u32.to_be_bytes());   // octets = 10
       let record = NetflowV5Record::from_bytes(&record_data).unwrap();
       assert!(record.validate().is_err());
       
       // Invalid: 1 packet, 100000 bytes (too large)
       record_data[20..24].copy_from_slice(&100000u32.to_be_bytes()); // octets = 100000
       let record = NetflowV5Record::from_bytes(&record_data).unwrap();
       assert!(record.validate().is_err());
   }

   #[test]
   fn test_tcp_flags_parsing() {
       let mut record_data = vec![0u8; NETFLOW_V5_RECORD_SIZE];
       
       // Set TCP protocol and flags
       record_data[38] = 6; // protocol = TCP
       record_data[37] = 0b00010010; // SYN + ACK flags
       
       let record = NetflowV5Record::from_bytes(&record_data).unwrap();
       let log_event = record.to_log_event(0, true);
       
       assert_eq!(log_event.get("tcp_flags_syn").unwrap().as_boolean().unwrap(), true);
       assert_eq!(log_event.get("tcp_flags_ack").unwrap().as_boolean().unwrap(), true);
       assert_eq!(log_event.get("tcp_flags_fin").unwrap().as_boolean().unwrap(), false);
       assert_eq!(log_event.get("tcp_flags_rst").unwrap().as_boolean().unwrap(), false);
       assert_eq!(log_event.get("tcp_flags_psh").unwrap().as_boolean().unwrap(), false);
       assert_eq!(log_event.get("tcp_flags_urg").unwrap().as_boolean().unwrap(), false);
   }

   #[test]
   fn test_dscp_ecn_parsing() {
       let mut record_data = vec![0u8; NETFLOW_V5_RECORD_SIZE];
       
       // Set TOS field: DSCP=26 (AF31), ECN=1
       record_data[39] = (26 << 2) | 1; // DSCP in upper 6 bits, ECN in lower 2
       
       let record = NetflowV5Record::from_bytes(&record_data).unwrap();
       let log_event = record.to_log_event(0, true);
       
       assert_eq!(log_event.get("tos").unwrap().as_integer().unwrap(), (26 << 2) | 1);
       assert_eq!(log_event.get("dscp").unwrap().as_integer().unwrap(), 26);
       assert_eq!(log_event.get("ecn").unwrap().as_integer().unwrap(), 1);
   }

   #[test]
   fn test_multiple_records_with_validation() {
       let config = NetflowConfig::default();
       let field_parser = FieldParser::new(&config);
       let parser = NetflowV5Parser::new(field_parser);

       // Create packet with mix of valid and invalid records
       let mut packet = create_netflow_v5_packet(3);
       
       // Make second record invalid (zero packets, non-zero octets)
       let second_record_offset = NETFLOW_V5_HEADER_SIZE + NETFLOW_V5_RECORD_SIZE;
       packet[second_record_offset + 16..second_record_offset + 20].copy_from_slice(&0u32.to_be_bytes()); // packets = 0
       packet[second_record_offset + 20..second_record_offset + 24].copy_from_slice(&1500u32.to_be_bytes()); // octets = 1500

       let events = parser.parse(&packet, test_peer_addr(), false).unwrap();

       // Should get 2 valid records (first and third)
       assert_eq!(events.len(), 2);
       
       // Check record numbers
       if let Event::Log(log) = &events[0] {
           assert_eq!(log.get("record_number").unwrap().as_integer().unwrap(), 0);
       }
       if let Event::Log(log) = &events[1] {
           assert_eq!(log.get("record_number").unwrap().as_integer().unwrap(), 2);
       }
   }

   #[test]
   fn test_protocol_name_resolution() {
       let mut record_data = vec![0u8; NETFLOW_V5_RECORD_SIZE];
       
       // Test various protocols
       let protocols = vec![
           (1, "ICMP"),
           (6, "TCP"),
           (17, "UDP"),
           (47, "GRE"),
           (50, "ESP"),
           (89, "OSPF"),
           (132, "SCTP"),
           (255, "Other"),
       ];
       
       for (proto_num, proto_name) in protocols {
           record_data[38] = proto_num;
           let record = NetflowV5Record::from_bytes(&record_data).unwrap();
           let log_event = record.to_log_event(0, true);
           
           assert_eq!(log_event.get("protocol").unwrap().as_integer().unwrap(), proto_num as i64);
           assert_eq!(log_event.get("protocol_name").unwrap().as_str().unwrap(), proto_name);
       }
   }

   #[test]
   fn test_header_to_log_event() {
       let packet = create_netflow_v5_packet(1);
       let header = NetflowV5Header::from_bytes(&packet).unwrap();
       let log_event = header.to_log_event();

       assert_eq!(log_event.get("flow_type").unwrap().as_str().unwrap(), "netflow_v5");
       assert_eq!(log_event.get("version").unwrap().as_integer().unwrap(), 5);
       assert_eq!(log_event.get("count").unwrap().as_integer().unwrap(), 1);
       assert_eq!(log_event.get("sys_uptime").unwrap().as_integer().unwrap(), 12345);
       assert_eq!(log_event.get("unix_secs").unwrap().as_integer().unwrap(), 1609459200);
       assert_eq!(log_event.get("flow_sequence").unwrap().as_integer().unwrap(), 100);
       assert_eq!(log_event.get("engine_type").unwrap().as_integer().unwrap(), 0);
       assert_eq!(log_event.get("engine_id").unwrap().as_integer().unwrap(), 0);
       assert_eq!(log_event.get("sampling_interval").unwrap().as_integer().unwrap(), 0);
   }

   #[test]
   fn test_ipv4_address_conversion() {
       assert_eq!(NetflowV5Record::ipv4_to_string(0xC0A80101), "192.168.1.1");
       assert_eq!(NetflowV5Record::ipv4_to_string(0x08080808), "8.8.8.8");
       assert_eq!(NetflowV5Record::ipv4_to_string(0x00000000), "0.0.0.0");
       assert_eq!(NetflowV5Record::ipv4_to_string(0xFFFFFFFF), "255.255.255.255");
   }
}