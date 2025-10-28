//! Protocol parsing coordination for NetFlow/IPFIX/sFlow.
//!
//! This module provides a unified interface for parsing different flow protocols
//! while maintaining protocol-specific logic in separate modules.

use crate::sources::netflow::config::NetflowConfig;
use crate::sources::netflow::events::*;
use crate::sources::netflow::fields::FieldParser;
use crate::sources::netflow::templates::TemplateCache;

use std::net::SocketAddr;
use tracing::{debug, warn};
use vector_lib::event::Event;


pub mod ipfix;
pub mod netflow_v5;
pub mod netflow_v9;
pub mod sflow;

pub use ipfix::IpfixParser;
pub use netflow_v5::NetflowV5Parser;
pub use netflow_v9::NetflowV9Parser;
pub use sflow::SflowParser;

/// Detected protocol type from packet analysis
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DetectedProtocol {
    NetflowV5,
    NetflowV9,
    Ipfix,
    Sflow,
    Unknown(u16), // Contains the version/type that was detected
}

impl DetectedProtocol {
    /// Get the protocol name as a string
    pub fn as_str(&self) -> &'static str {
        match self {
            DetectedProtocol::NetflowV5 => "netflow_v5",
            DetectedProtocol::NetflowV9 => "netflow_v9",
            DetectedProtocol::Ipfix => "ipfix",
            DetectedProtocol::Sflow => "sflow",
            DetectedProtocol::Unknown(_) => "unknown",
        }
    }

    /// Check if this protocol is enabled in configuration
    pub fn is_enabled(&self, config: &NetflowConfig) -> bool {
        let flow_protocol = match self {
            DetectedProtocol::NetflowV5 => "netflow_v5",
            DetectedProtocol::NetflowV9 => "netflow_v9",
            DetectedProtocol::Ipfix => "ipfix",
            DetectedProtocol::Sflow => "sflow",
            DetectedProtocol::Unknown(_) => return false,
        };
        
        config.is_protocol_enabled(flow_protocol)
    }
}

/// Main protocol parser that coordinates all flow protocol parsers
pub struct ProtocolParser {
    netflow_v5: NetflowV5Parser,
    netflow_v9: NetflowV9Parser,
    ipfix: IpfixParser,
    sflow: SflowParser,
    enabled_protocols: Vec<String>,
}

impl ProtocolParser {
    /// Create a new protocol parser with the given configuration
    pub fn new(config: &NetflowConfig, _template_cache: TemplateCache) -> Self {
        let field_parser = FieldParser::new(config);

        Self {
            netflow_v5: NetflowV5Parser::new(field_parser.clone(), config.strict_validation),
            netflow_v9: NetflowV9Parser::new(field_parser.clone()),
            ipfix: IpfixParser::new(field_parser.clone(), config.options_template_mode.clone()),
            sflow: SflowParser::new(),
            enabled_protocols: config.protocols.iter().map(|s| s.to_string()).collect(),
        }
    }

    /// Parse a packet and return flow events
    pub fn parse(&self, data: &[u8], peer_addr: SocketAddr, template_cache: &TemplateCache) -> Vec<Event> {
        // Detect protocol type
        let protocol = self.detect_protocol(data);
        
        debug!(
            "Detected protocol: {} from peer {}",
            protocol.as_str(),
            peer_addr
        );

        // Check if protocol is enabled
        let config_stub = NetflowConfig {
            protocols: self.enabled_protocols.clone(),
            ..Default::default()
        };

        if !protocol.is_enabled(&config_stub) {
            debug!(
                "Protocol {} is disabled, ignoring packet from {}",
                protocol.as_str(),
                peer_addr
            );
            
            emit!(ProtocolDisabled {
                protocol: protocol.as_str(),
                peer_addr,
            });
            
            // For unknown protocols, still generate an event
            if let DetectedProtocol::Unknown(version) = protocol {
                return vec![self.create_unknown_protocol_event(data, peer_addr, version)];
            }
            
            return Vec::new();
        }

        // Parse using appropriate parser
        let parse_result = match protocol {
            DetectedProtocol::NetflowV5 => {
                self.netflow_v5.parse(
                    data,
                    peer_addr,
                    true, // include_raw_data
                )
            }
            DetectedProtocol::NetflowV9 => {
                self.netflow_v9.parse(
                    data,
                    peer_addr,
                    template_cache,
                    false, // include_raw_data
                    true,  // drop_unparseable_records
                )
            }
            DetectedProtocol::Ipfix => {
                self.ipfix.parse(
                    data,
                    peer_addr,
                    template_cache,
                    false, // include_raw_data
                    true,  // drop_unparseable_records
                    true,  // buffer_missing_templates
                )
            }
            DetectedProtocol::Sflow => {
                self.sflow.parse(
                    data,
                    peer_addr,
                    false, // include_raw_data
                )
            }
            DetectedProtocol::Unknown(version) => {
                Ok(vec![self.create_unknown_protocol_event(data, peer_addr, version)])
            }
        };

        match parse_result {
            Ok(events) => {
                if !events.is_empty() {
                    emit!(ProtocolParseSuccess {
                        protocol: protocol.as_str(),
                        peer_addr,
                        event_count: events.len(),
                        byte_size: data.len(),
                    });
                }
                events
            }
            Err(error) => {
                emit!(NetflowParseError {
                    error: &error,
                    protocol: protocol.as_str(),
                    peer_addr,
                });

                // Return basic event with error info instead of empty
                vec![self.create_parse_error_event(data, peer_addr, &protocol, &error)]
            }
        }
    }

    /// Detect the protocol type from packet data
    fn detect_protocol(&self, data: &[u8]) -> DetectedProtocol {
        if data.len() < 2 {
            return DetectedProtocol::Unknown(0);
        }

        // Check NetFlow/IPFIX version (first 2 bytes, big-endian)
        let version = u16::from_be_bytes([data[0], data[1]]);
        
        match version {
            5 => {
                // NetFlow v5 - verify packet structure
                if NetflowV5Parser::can_parse(data) {
                    DetectedProtocol::NetflowV5
                } else {
                    DetectedProtocol::Unknown(version)
                }
            }
            9 => {
                // NetFlow v9 - verify packet structure
                if NetflowV9Parser::can_parse(data) {
                    DetectedProtocol::NetflowV9
                } else {
                    DetectedProtocol::Unknown(version)
                }
            }
            10 => {
                // IPFIX - verify packet structure
                if IpfixParser::can_parse(data) {
                    DetectedProtocol::Ipfix
                } else {
                    DetectedProtocol::Unknown(version)
                }
            }
            _ => {
                // Check for sFlow (version is at different offset)
                if data.len() >= 4 {
                    let sflow_version = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);
                    if sflow_version == 5 && SflowParser::can_parse(data) {
                        return DetectedProtocol::Sflow;
                    }
                }
                
                DetectedProtocol::Unknown(version)
            }
        }
    }

    /// Create an event for unknown protocol packets
    fn create_unknown_protocol_event(
        &self,
        data: &[u8],
        peer_addr: SocketAddr,
        version: u16,
    ) -> Event {
        let mut log_event = vector_lib::event::LogEvent::default();
        
        log_event.insert("flow_type", "unknown");
        log_event.insert("version", version);
        log_event.insert("peer_addr", peer_addr.to_string());
        log_event.insert("packet_length", data.len());
        
        // Include some packet analysis
        if data.len() >= 4 {
            log_event.insert("first_4_bytes", hex::encode(&data[..4]));
        }
        
        if data.len() >= 8 {
            log_event.insert("first_8_bytes", hex::encode(&data[..8]));
        }
        
        // Include raw data if configured
        // Removed include_raw_data, so this block is removed.

        Event::Log(log_event)
    }

    /// Create an event for parse errors
    fn create_parse_error_event(
        &self,
        data: &[u8],
        peer_addr: SocketAddr,
        protocol: &DetectedProtocol,
        error: &str,
    ) -> Event {
        let mut log_event = vector_lib::event::LogEvent::default();
        
        log_event.insert("flow_type", "parse_error");
        log_event.insert("detected_protocol", protocol.as_str());
        log_event.insert("peer_addr", peer_addr.to_string());
        log_event.insert("packet_length", data.len());
        log_event.insert("parse_error", error);
        
        if let DetectedProtocol::Unknown(version) = protocol {
            log_event.insert("detected_version", *version);
        }
        
        // Include packet header for debugging
        if data.len() >= 16 {
            log_event.insert("packet_header", hex::encode(&data[..16]));
        } else {
            log_event.insert("packet_header", hex::encode(data));
        }
        
        // Include raw data if configured
        // Removed include_raw_data, so this block is removed.

        Event::Log(log_event)
    }

    /// Get statistics about supported protocols
    pub fn get_protocol_stats(&self) -> ProtocolStats {
        ProtocolStats {
            enabled_protocols: self.enabled_protocols.clone(),
            total_enabled: self.enabled_protocols.len(),
        }
    }
}

/// Statistics about protocol support
#[derive(Debug, Clone)]
pub struct ProtocolStats {
    pub enabled_protocols: Vec<String>,
    pub total_enabled: usize,
}

/// Parse flow data using the protocol parser - main entry point
pub fn parse_flow_data(
    data: &[u8],
    peer_addr: SocketAddr,
    template_cache: &TemplateCache,
    config: &NetflowConfig,
) -> Result<Vec<Event>, String> {
    if data.is_empty() {
        return Err("Empty packet received".to_string());
    }

    let parser = ProtocolParser::new(config, template_cache.clone());
    let events = parser.parse(data, peer_addr, template_cache);
    
    Ok(events)
}

/// Additional internal events for protocol coordination
#[derive(Debug)]
pub struct ProtocolDisabled {
    pub protocol: &'static str,
    pub peer_addr: SocketAddr,
}

impl vector_lib::internal_event::InternalEvent for ProtocolDisabled {
    fn emit(self) {
        debug!(
            message = "Protocol disabled, ignoring packet",
            protocol = self.protocol,
            peer_addr = %self.peer_addr,
        );
    }
}

#[derive(Debug)]
pub struct ProtocolParseSuccess {
    pub protocol: &'static str,
    pub peer_addr: SocketAddr,
    pub event_count: usize,
    pub byte_size: usize,
}

impl vector_lib::internal_event::InternalEvent for ProtocolParseSuccess {
    fn emit(self) {
        debug!(
            message = "Protocol parsed successfully",
            protocol = self.protocol,
            peer_addr = %self.peer_addr,
            event_count = self.event_count,
            byte_size = self.byte_size,
        );
        
        // Emit metrics
        // Metrics are handled by the ComponentEventsReceived event
    }
}

#[derive(Debug)]
pub struct ProtocolDetectionFailed {
    pub peer_addr: SocketAddr,
    pub packet_length: usize,
    pub first_bytes: String,
}

impl vector_lib::internal_event::InternalEvent for ProtocolDetectionFailed {
    fn emit(self) {
        warn!(
            message = "Failed to detect protocol",
            peer_addr = %self.peer_addr,
            packet_length = self.packet_length,
            first_bytes = %self.first_bytes,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sources::netflow::config::NetflowConfig;
    use crate::sources::netflow::templates::TemplateCache;
    use std::net::{IpAddr, Ipv4Addr};

    fn test_peer_addr() -> SocketAddr {
        SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 100)), 2055)
    }

    fn test_config() -> NetflowConfig {
        NetflowConfig::default()
    }

    fn create_netflow_v5_packet() -> Vec<u8> {
        let mut packet = vec![0u8; 72]; // 24 header + 48 record
        packet[0..2].copy_from_slice(&5u16.to_be_bytes()); // version
        packet[2..4].copy_from_slice(&1u16.to_be_bytes()); // count
        packet[4..8].copy_from_slice(&12345u32.to_be_bytes()); // sys_uptime
        packet
    }

    fn create_netflow_v9_packet() -> Vec<u8> {
        let mut packet = vec![0u8; 20]; // Just header
        packet[0..2].copy_from_slice(&9u16.to_be_bytes()); // version
        packet[2..4].copy_from_slice(&0u16.to_be_bytes()); // count
        packet
    }

    fn create_ipfix_packet() -> Vec<u8> {
        let mut packet = vec![0u8; 16]; // IPFIX header
        packet[0..2].copy_from_slice(&10u16.to_be_bytes()); // version
        packet[2..4].copy_from_slice(&16u16.to_be_bytes()); // length
        packet
    }

    fn create_sflow_packet() -> Vec<u8> {
        let mut packet = vec![0u8; 28]; // sFlow header
        packet[0..4].copy_from_slice(&5u32.to_be_bytes()); // version
        packet[4..8].copy_from_slice(&1u32.to_be_bytes()); // agent_address_type (IPv4)
        packet[24..28].copy_from_slice(&0u32.to_be_bytes()); // num_samples
        packet
    }

    #[test]
    fn test_protocol_detection() {
        let template_cache = TemplateCache::new(100);
        let config = test_config();
        let parser = ProtocolParser::new(&config, template_cache);

        // Test NetFlow v5
        let nf5_packet = create_netflow_v5_packet();
        let protocol = parser.detect_protocol(&nf5_packet);
        assert_eq!(protocol, DetectedProtocol::NetflowV5);

        // Test NetFlow v9
        let nf9_packet = create_netflow_v9_packet();
        let protocol = parser.detect_protocol(&nf9_packet);
        assert_eq!(protocol, DetectedProtocol::NetflowV9);

        // Test IPFIX
        let ipfix_packet = create_ipfix_packet();
        let protocol = parser.detect_protocol(&ipfix_packet);
        assert_eq!(protocol, DetectedProtocol::Ipfix);

        // Test sFlow
        let sflow_packet = create_sflow_packet();
        let protocol = parser.detect_protocol(&sflow_packet);
        assert_eq!(protocol, DetectedProtocol::Sflow);

        // Test unknown protocol
        let unknown_packet = vec![0xFF, 0xFF, 0x00, 0x00];
        let protocol = parser.detect_protocol(&unknown_packet);
        assert_eq!(protocol, DetectedProtocol::Unknown(0xFFFF));
    }

    #[test]
    fn test_protocol_enabled_check() {
        let config = NetflowConfig {
            protocols: vec!["netflow_v5".to_string(), "ipfix".to_string()],
            ..Default::default()
        };

        assert!(DetectedProtocol::NetflowV5.is_enabled(&config));
        assert!(DetectedProtocol::Ipfix.is_enabled(&config));
        assert!(!DetectedProtocol::NetflowV9.is_enabled(&config));
        assert!(!DetectedProtocol::Sflow.is_enabled(&config));
        assert!(!DetectedProtocol::Unknown(123).is_enabled(&config));
    }

    #[test]
    fn test_parse_disabled_protocol() {
        let template_cache = TemplateCache::new(100);
        let config = NetflowConfig {
            protocols: vec!["ipfix".to_string()], // Only IPFIX enabled
            ..Default::default()
        };
        let parser = ProtocolParser::new(&config, template_cache.clone());

        // Try to parse NetFlow v5 (disabled)
        let nf5_packet = create_netflow_v5_packet();
        let events = parser.parse(&nf5_packet, test_peer_addr(), &template_cache);
        
        // Should return empty (protocol disabled)
        assert!(events.is_empty());
    }

    #[test]
    fn test_parse_enabled_protocol() {
        let template_cache = TemplateCache::new(100);
        let config = test_config(); // All protocols enabled
        let parser = ProtocolParser::new(&config, template_cache.clone());

        // Parse NetFlow v5
        let nf5_packet = create_netflow_v5_packet();
        let events = parser.parse(&nf5_packet, test_peer_addr(), &template_cache);
        
        // Should return events
        assert!(!events.is_empty());
        if let Event::Log(log) = &events[0] {
            assert_eq!(log.get("flow_type").unwrap().as_str().unwrap(), "netflow_v5_record");
        }
    }

    #[test]
    fn test_unknown_protocol_event() {
        let template_cache = TemplateCache::new(100);
        let config = test_config();
        let parser = ProtocolParser::new(&config, template_cache.clone());

        // Create unknown protocol packet
        let unknown_packet = vec![0x99, 0x99, 0x00, 0x00, 0x11, 0x22, 0x33, 0x44];
        let events = parser.parse(&unknown_packet, test_peer_addr(), &template_cache);

        assert!(!events.is_empty());
        if let Event::Log(log) = &events[0] {
            assert_eq!(log.get("flow_type").unwrap().as_str().unwrap(), "unknown");
            assert_eq!(log.get("version").unwrap().as_integer().unwrap(), 0x9999);
            assert!(log.get("first_4_bytes").is_some());
            assert!(log.get("first_8_bytes").is_some());
        }
    }

    #[test]
    fn test_parse_error_event() {
        let template_cache = TemplateCache::new(100);
        let config = test_config();
        let parser = ProtocolParser::new(&config, template_cache.clone());

        // Create malformed IPFIX packet (detected as IPFIX but fails during parsing)
        // We'll create a packet that claims to be longer than it actually is
        let mut malformed_packet = vec![0u8; 32]; // IPFIX header size
        malformed_packet[0..2].copy_from_slice(&10u16.to_be_bytes()); // version (IPFIX)
        malformed_packet[2..4].copy_from_slice(&64u16.to_be_bytes()); // length = 64 (but packet is only 32 bytes)
        malformed_packet[4..8].copy_from_slice(&12345u32.to_be_bytes()); // export time
        malformed_packet[8..12].copy_from_slice(&1u32.to_be_bytes()); // sequence number
        malformed_packet[12..16].copy_from_slice(&1u32.to_be_bytes()); // observation domain
        // The packet claims to be 64 bytes but is only 32 bytes, which should cause parsing to fail
        let events = parser.parse(&malformed_packet, test_peer_addr(), &template_cache);

        assert!(!events.is_empty());
        if let Event::Log(log) = &events[0] {
            assert_eq!(log.get("flow_type").unwrap().as_str().unwrap(), "parse_error");
            assert_eq!(log.get("detected_protocol").unwrap().as_str().unwrap(), "ipfix");
            assert!(log.get("parse_error").is_some());
        }
    }

    #[test]
    fn test_raw_data_inclusion() {
        let template_cache = TemplateCache::new(100);
        let config = NetflowConfig {
            protocols: vec!["netflow_v5".to_string()],
            ..Default::default()
        };
        let parser = ProtocolParser::new(&config, template_cache.clone());

        let nf5_packet = create_netflow_v5_packet();
        let events = parser.parse(&nf5_packet, test_peer_addr(), &template_cache);

        assert!(!events.is_empty());
        if let Event::Log(log) = &events[0] {
            assert!(log.get("raw_data").is_some());
        }
    }

    #[test]
    fn test_empty_packet() {
        let template_cache = TemplateCache::new(100);
        let config = test_config();

        let result = parse_flow_data(&[], test_peer_addr(), &template_cache, &config);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Empty packet received");
    }

    #[test]
    fn test_protocol_stats() {
        let template_cache = TemplateCache::new(100);
        let config = NetflowConfig {
            protocols: vec!["netflow_v5".to_string(), "ipfix".to_string()],
            ..Default::default()
        };
        let parser = ProtocolParser::new(&config, template_cache);

        let stats = parser.get_protocol_stats();
        assert_eq!(stats.total_enabled, 2);
        assert!(stats.enabled_protocols.contains(&"netflow_v5".to_string()));
        assert!(stats.enabled_protocols.contains(&"ipfix".to_string()));
    }

    #[test]
    fn test_protocol_as_str() {
        assert_eq!(DetectedProtocol::NetflowV5.as_str(), "netflow_v5");
        assert_eq!(DetectedProtocol::NetflowV9.as_str(), "netflow_v9");
        assert_eq!(DetectedProtocol::Ipfix.as_str(), "ipfix");
        assert_eq!(DetectedProtocol::Sflow.as_str(), "sflow");
        assert_eq!(DetectedProtocol::Unknown(123).as_str(), "unknown");
    }

    #[test]
    fn test_short_packet_detection() {
        let template_cache = TemplateCache::new(100);
        let config = test_config();
        let parser = ProtocolParser::new(&config, template_cache);

        // Single byte packet
        let short_packet = vec![0x05];
        let protocol = parser.detect_protocol(&short_packet);
        assert_eq!(protocol, DetectedProtocol::Unknown(0));

        // Empty packet
        let empty_packet = vec![];
        let protocol = parser.detect_protocol(&empty_packet);
        assert_eq!(protocol, DetectedProtocol::Unknown(0));
    }

    #[test]
    fn test_main_parse_function() {
        let template_cache = TemplateCache::new(100);
        let config = test_config();

        let nf5_packet = create_netflow_v5_packet();
        let result = parse_flow_data(&nf5_packet, test_peer_addr(), &template_cache, &config);

        assert!(result.is_ok());
        let events = result.unwrap();
        assert!(!events.is_empty());
    }
}