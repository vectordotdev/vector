//! Protocol parsing for NetFlow v5.
//!
//! This module provides parsing for NetFlow v5 flow records.

use std::net::SocketAddr;

use tracing::debug;
use vector_lib::event::Event;

use crate::sources::netflow::config::NetflowConfig;
use crate::sources::netflow::events::*;
use crate::sources::netflow::fields::FieldParser;
use crate::sources::netflow::templates::TemplateCache;

pub mod netflow_v5;

pub use netflow_v5::NetflowV5Parser;

/// Detected protocol type from packet analysis.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DetectedProtocol {
    NetflowV5,
    Unknown(u16),
}

impl DetectedProtocol {
    /// Get the protocol name as a string.
    pub fn as_str(&self) -> &'static str {
        match self {
            DetectedProtocol::NetflowV5 => "netflow_v5",
            DetectedProtocol::Unknown(_) => "unknown",
        }
    }

    /// Check if this protocol is enabled in configuration.
    pub fn is_enabled(&self, config: &NetflowConfig) -> bool {
        let flow_protocol = match self {
            DetectedProtocol::NetflowV5 => "netflow_v5",
            DetectedProtocol::Unknown(_) => return false,
        };
        config.is_protocol_enabled(flow_protocol)
    }
}

/// Protocol parser for NetFlow v5.
pub struct ProtocolParser {
    netflow_v5: NetflowV5Parser,
    enabled_protocols: Vec<String>,
    include_raw_data: bool,
}

impl ProtocolParser {
    /// Create a new protocol parser with the given configuration.
    pub fn new(config: &NetflowConfig, _template_cache: TemplateCache) -> Self {
        let field_parser = FieldParser::new(config);
        Self {
            netflow_v5: NetflowV5Parser::new(field_parser.clone(), config.strict_validation),
            enabled_protocols: config.protocols.iter().map(|s| s.to_string()).collect(),
            include_raw_data: config.include_raw_data,
        }
    }

    /// Parse a packet and return flow events.
    pub fn parse(&self, data: &[u8], peer_addr: SocketAddr, _template_cache: &TemplateCache) -> Vec<Event> {
        let protocol = self.detect_protocol(data);

        debug!(
            message = "Detected protocol.",
            protocol = protocol.as_str(),
            peer_addr = %peer_addr,
        );

        let config_stub = NetflowConfig {
            protocols: self.enabled_protocols.clone(),
            ..Default::default()
        };

        if !protocol.is_enabled(&config_stub) {
            debug!(
                message = "Protocol disabled, ignoring packet.",
                protocol = protocol.as_str(),
                peer_addr = %peer_addr,
            );
            emit!(ProtocolDisabled {
                protocol: protocol.as_str(),
                peer_addr,
            });
            if let DetectedProtocol::Unknown(version) = protocol {
                return vec![self.create_unknown_protocol_event(data, peer_addr, version)];
            }
            return Vec::new();
        }

        let parse_result = match protocol {
            DetectedProtocol::NetflowV5 => {
                self.netflow_v5.parse(data, peer_addr, self.include_raw_data)
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
                vec![self.create_parse_error_event(data, peer_addr, &protocol, &error)]
            }
        }
    }

    fn detect_protocol(&self, data: &[u8]) -> DetectedProtocol {
        if data.len() < 2 {
            return DetectedProtocol::Unknown(0);
        }
        let version = u16::from_be_bytes([data[0], data[1]]);
        if version == 5 && NetflowV5Parser::can_parse(data) {
            DetectedProtocol::NetflowV5
        } else {
            DetectedProtocol::Unknown(version)
        }
    }

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
        if data.len() >= 4 {
            log_event.insert("first_4_bytes", hex::encode(&data[..4]));
        }
        if data.len() >= 8 {
            log_event.insert("first_8_bytes", hex::encode(&data[..8]));
        }
        Event::Log(log_event)
    }

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
        if data.len() >= 16 {
            log_event.insert("packet_header", hex::encode(&data[..16]));
        } else {
            log_event.insert("packet_header", hex::encode(data));
        }
        Event::Log(log_event)
    }

    /// Get statistics about supported protocols.
    pub fn get_protocol_stats(&self) -> ProtocolStats {
        ProtocolStats {
            enabled_protocols: self.enabled_protocols.clone(),
            total_enabled: self.enabled_protocols.len(),
        }
    }
}

/// Statistics about protocol support.
#[derive(Debug, Clone)]
pub struct ProtocolStats {
    pub enabled_protocols: Vec<String>,
    pub total_enabled: usize,
}

/// Parse flow data using the protocol parser.
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
    Ok(parser.parse(data, peer_addr, template_cache))
}

#[derive(Debug)]
pub struct ProtocolDisabled {
    pub protocol: &'static str,
    pub peer_addr: SocketAddr,
}

impl vector_lib::internal_event::InternalEvent for ProtocolDisabled {
    fn emit(self) {
        debug!(
            message = "Protocol disabled, ignoring packet.",
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
            message = "Protocol parsed successfully.",
            protocol = self.protocol,
            peer_addr = %self.peer_addr,
            event_count = self.event_count,
            byte_size = self.byte_size,
        );
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
        tracing::warn!(
            message = "Failed to detect protocol.",
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
        let mut packet = vec![0u8; 72];
        packet[0..2].copy_from_slice(&5u16.to_be_bytes());
        packet[2..4].copy_from_slice(&1u16.to_be_bytes());
        packet[4..8].copy_from_slice(&12345u32.to_be_bytes());
        packet
    }

    #[test]
    fn test_protocol_detection() {
        let template_cache = TemplateCache::new(100);
        let config = test_config();
        let parser = ProtocolParser::new(&config, template_cache);

        let nf5_packet = create_netflow_v5_packet();
        let protocol = parser.detect_protocol(&nf5_packet);
        assert_eq!(protocol, DetectedProtocol::NetflowV5);

        let unknown_packet = vec![0xFF, 0xFF, 0x00, 0x00];
        let protocol = parser.detect_protocol(&unknown_packet);
        assert_eq!(protocol, DetectedProtocol::Unknown(0xFFFF));
    }

    #[test]
    fn test_protocol_enabled_check() {
        let config = NetflowConfig {
            protocols: vec!["netflow_v5".to_string()],
            ..Default::default()
        };
        assert!(DetectedProtocol::NetflowV5.is_enabled(&config));
        assert!(!DetectedProtocol::Unknown(123).is_enabled(&config));
    }

    #[test]
    fn test_parse_disabled_protocol() {
        let template_cache = TemplateCache::new(100);
        let config = NetflowConfig {
            protocols: vec![], // No protocols enabled
            ..Default::default()
        };
        let parser = ProtocolParser::new(&config, template_cache.clone());
        let nf5_packet = create_netflow_v5_packet();
        let events = parser.parse(&nf5_packet, test_peer_addr(), &template_cache);
        assert!(events.is_empty());
    }

    #[test]
    fn test_parse_enabled_protocol() {
        let template_cache = TemplateCache::new(100);
        let config = test_config();
        let parser = ProtocolParser::new(&config, template_cache.clone());

        let nf5_packet = create_netflow_v5_packet();
        let events = parser.parse(&nf5_packet, test_peer_addr(), &template_cache);

        assert!(!events.is_empty());
        if let Event::Log(log) = &events[0] {
            assert_eq!(log.get("flow_type").unwrap().as_str().unwrap(), "netflow_v5");
        }
    }

    #[test]
    fn test_unknown_protocol_event() {
        let template_cache = TemplateCache::new(100);
        let config = test_config();
        let parser = ProtocolParser::new(&config, template_cache.clone());

        let unknown_packet = vec![0x99, 0x99, 0x00, 0x00, 0x11, 0x22, 0x33, 0x44];
        let events = parser.parse(&unknown_packet, test_peer_addr(), &template_cache);

        assert!(!events.is_empty());
        if let Event::Log(log) = &events[0] {
            assert_eq!(log.get("flow_type").unwrap().as_str().unwrap(), "unknown");
            assert_eq!(log.get("version").unwrap().as_integer().unwrap(), 0x9999);
        }
    }

    #[test]
    fn test_raw_data_inclusion() {
        let template_cache = TemplateCache::new(100);
        let config = NetflowConfig {
            protocols: vec!["netflow_v5".to_string()],
            include_raw_data: true,
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
    fn test_raw_data_excluded_by_default() {
        let template_cache = TemplateCache::new(100);
        let config = NetflowConfig::default();
        let parser = ProtocolParser::new(&config, template_cache.clone());

        let nf5_packet = create_netflow_v5_packet();
        let events = parser.parse(&nf5_packet, test_peer_addr(), &template_cache);

        assert!(!events.is_empty());
        if let Event::Log(log) = &events[0] {
            assert!(log.get("raw_data").is_none());
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
            protocols: vec!["netflow_v5".to_string()],
            ..Default::default()
        };
        let parser = ProtocolParser::new(&config, template_cache);
        let stats = parser.get_protocol_stats();
        assert_eq!(stats.total_enabled, 1);
        assert!(stats.enabled_protocols.contains(&"netflow_v5".to_string()));
    }

    #[test]
    fn test_protocol_as_str() {
        assert_eq!(DetectedProtocol::NetflowV5.as_str(), "netflow_v5");
        assert_eq!(DetectedProtocol::Unknown(123).as_str(), "unknown");
    }

    #[test]
    fn test_short_packet_detection() {
        let template_cache = TemplateCache::new(100);
        let config = test_config();
        let parser = ProtocolParser::new(&config, template_cache);

        let short_packet = vec![0x05];
        let protocol = parser.detect_protocol(&short_packet);
        assert_eq!(protocol, DetectedProtocol::Unknown(0));

        let empty_packet: Vec<u8> = vec![];
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
