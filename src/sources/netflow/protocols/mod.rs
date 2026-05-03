//! NetFlow packet decoding for this source.
//!
//! Currently implements NetFlow version 5 only.

use std::{
    collections::{HashMap, HashSet},
    net::{IpAddr, SocketAddr},
};

use tracing::debug;
use vector_lib::event::Event;

use crate::sources::netflow::config::NetflowConfig;
use crate::sources::netflow::events::*;
use crate::sources::netflow::fields::FieldParser;
use crate::sources::netflow::templates::TemplateCache;

pub mod netflow_v5;

pub use netflow_v5::NetflowV5Parser;

/// Protocol discriminator based on the datagram version word.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DetectedProtocol {
    NetflowV5,
    Unknown(u16),
}

impl DetectedProtocol {
    /// Stable name used in metrics and diagnostic logs.
    pub fn as_str(&self) -> &'static str {
        match self {
            DetectedProtocol::NetflowV5 => "netflow_v5",
            DetectedProtocol::Unknown(_) => "unknown",
        }
    }
}

/// Dispatches UDP payloads to the NetFlow v5 parser.
pub struct ProtocolParser {
    netflow_v5: NetflowV5Parser,
    enabled_protocols: HashSet<String>,
    include_raw_data: bool,
}

impl ProtocolParser {
    /// Builds a parser from configuration.
    ///
    /// `_template_cache` is unused for NetFlow v5 but threaded through from the source for a stable API.
    pub fn new(config: &NetflowConfig, _template_cache: TemplateCache) -> Self {
        let field_parser = FieldParser::new(config);
        let enabled_protocols = config.protocols.iter().cloned().collect();
        Self {
            netflow_v5: NetflowV5Parser::new(field_parser, config.strict_validation),
            enabled_protocols,
            include_raw_data: config.include_raw_data,
        }
    }

    /// True when `name` appears in [`NetflowConfig::protocols`].
    fn protocol_enabled(&self, name: &str) -> bool {
        self.enabled_protocols.contains(name)
    }

    /// Decodes one datagram and appends to `out`. Returns the number of [`Event`] values appended.
    ///
    /// `sequence_tracker` must be the per-worker map used for NetFlow v5 sequence-gap hints.
    pub fn parse_into(
        &self,
        data: &[u8],
        peer_addr: SocketAddr,
        _template_cache: &TemplateCache,
        sequence_tracker: &mut HashMap<IpAddr, (u32, u16)>,
        out: &mut Vec<Event>,
    ) -> usize {
        let start_len = out.len();
        let protocol = self.detect_protocol(data);

        debug!(
            message = "Detected protocol.",
            protocol = protocol.as_str(),
            peer_addr = %peer_addr,
        );

        if !self.protocol_enabled(protocol.as_str()) {
            debug!(
                message = "Protocol disabled or unknown, emitting diagnostic event.",
                protocol = protocol.as_str(),
                peer_addr = %peer_addr,
            );
            emit!(ProtocolDisabled {
                protocol: protocol.as_str(),
                peer_addr,
            });
            match protocol {
                DetectedProtocol::Unknown(version) => {
                    out.push(self.create_unknown_protocol_event(data, peer_addr, version));
                }
                _ => {
                    out.push(self.create_disabled_protocol_event(
                        data,
                        peer_addr,
                        protocol.as_str(),
                    ));
                }
            }
            return out.len() - start_len;
        }

        match protocol {
            DetectedProtocol::NetflowV5 => {
                match self.netflow_v5.parse_into(
                    data,
                    peer_addr,
                    self.include_raw_data,
                    sequence_tracker,
                    out,
                ) {
                    Ok(()) => {
                        let added = out.len() - start_len;
                        if added > 0 {
                            emit!(ProtocolParseSuccess {
                                protocol: protocol.as_str(),
                                peer_addr,
                                event_count: added,
                                byte_size: data.len(),
                            });
                        }
                        added
                    }
                    Err(error) => {
                        emit!(NetflowParseError {
                            error: &error,
                            protocol: protocol.as_str(),
                            peer_addr,
                        });
                        out.push(self.create_parse_error_event(data, peer_addr, &protocol, &error));
                        out.len() - start_len
                    }
                }
            }
            DetectedProtocol::Unknown(version) => {
                out.push(self.create_unknown_protocol_event(data, peer_addr, version));
                let added = out.len() - start_len;
                if added > 0 {
                    emit!(ProtocolParseSuccess {
                        protocol: protocol.as_str(),
                        peer_addr,
                        event_count: added,
                        byte_size: data.len(),
                    });
                }
                added
            }
        }
    }

    /// Decodes one datagram into a new [`Vec`]; prefer [`Self::parse_into`] on hot paths.
    pub fn parse(
        &self,
        data: &[u8],
        peer_addr: SocketAddr,
        template_cache: &TemplateCache,
        sequence_tracker: &mut HashMap<IpAddr, (u32, u16)>,
    ) -> Vec<Event> {
        let mut out = Vec::new();
        let _ = self.parse_into(data, peer_addr, template_cache, sequence_tracker, &mut out);
        out
    }

    fn detect_protocol(&self, data: &[u8]) -> DetectedProtocol {
        if data.len() < 2 {
            return DetectedProtocol::Unknown(0);
        }
        let version = u16::from_be_bytes([data[0], data[1]]);
        if version == 5 && NetflowV5Parser::can_parse_with_version(data, version) {
            DetectedProtocol::NetflowV5
        } else {
            DetectedProtocol::Unknown(version)
        }
    }

    // Diagnostic events are NOT NetFlow v5 flow records.
    // They use operational field names and are emitted only on
    // error/disabled paths. Consumers should filter on `flow_type`
    // to distinguish from real flow records.

    fn create_disabled_protocol_event(
        &self,
        data: &[u8],
        peer_addr: SocketAddr,
        protocol: &str,
    ) -> Event {
        let mut log_event = vector_lib::event::LogEvent::default();
        log_event.insert("flow_type", "disabled");
        log_event.insert("protocol", protocol);
        log_event.insert("peer_addr", peer_addr.to_string());
        log_event.insert("packet_length", data.len());
        if data.len() >= 4 {
            log_event.insert("first_4_bytes", hex::encode(&data[..4]));
        }
        Event::Log(log_event)
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

    /// Lists enabled protocol names from configuration.
    pub fn get_protocol_stats(&self) -> ProtocolStats {
        ProtocolStats {
            enabled_protocols: self.enabled_protocols.iter().cloned().collect(),
            total_enabled: self.enabled_protocols.len(),
        }
    }
}

/// Enabled protocol names derived from configuration.
#[derive(Debug, Clone)]
pub struct ProtocolStats {
    pub enabled_protocols: Vec<String>,
    pub total_enabled: usize,
}

/// Parses non-empty `data` through `parser`, or returns an error for empty input.
pub fn parse_flow_data(
    parser: &ProtocolParser,
    data: &[u8],
    peer_addr: SocketAddr,
    template_cache: &TemplateCache,
    sequence_tracker: &mut HashMap<IpAddr, (u32, u16)>,
) -> Result<Vec<Event>, String> {
    if data.is_empty() {
        return Err("Empty packet received".to_string());
    }
    Ok(parser.parse(data, peer_addr, template_cache, sequence_tracker))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sources::netflow::config::NetflowConfig;
    use crate::sources::netflow::templates::TemplateCache;
    use std::collections::HashMap;
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
        assert!(config.is_protocol_enabled("netflow_v5"));
        assert!(!config.is_protocol_enabled("unknown"));
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
        let mut seq = HashMap::new();
        let events = parser.parse(&nf5_packet, test_peer_addr(), &template_cache, &mut seq);
        assert_eq!(events.len(), 1);
        let log = events[0].as_log();
        assert_eq!(log.get("flow_type").unwrap().as_str().unwrap(), "disabled");
        assert_eq!(log.get("protocol").unwrap().as_str().unwrap(), "netflow_v5");
    }

    #[test]
    fn test_parse_enabled_protocol() {
        let template_cache = TemplateCache::new(100);
        let config = test_config();
        let parser = ProtocolParser::new(&config, template_cache.clone());

        let nf5_packet = create_netflow_v5_packet();
        let mut seq = HashMap::new();
        let events = parser.parse(&nf5_packet, test_peer_addr(), &template_cache, &mut seq);

        assert!(!events.is_empty());
        if let Event::Log(log) = &events[0] {
            assert_eq!(
                log.get("srcaddr").unwrap().as_str().unwrap(),
                "0.0.0.0"
            );
            assert_eq!(log.get("version").unwrap().as_integer().unwrap(), 5);
        }
    }

    #[test]
    fn test_unknown_protocol_event() {
        let template_cache = TemplateCache::new(100);
        let config = test_config();
        let parser = ProtocolParser::new(&config, template_cache.clone());

        let unknown_packet = vec![0x99, 0x99, 0x00, 0x00, 0x11, 0x22, 0x33, 0x44];
        let mut seq = HashMap::new();
        let events = parser.parse(
            &unknown_packet,
            test_peer_addr(),
            &template_cache,
            &mut seq,
        );

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
        let mut seq = HashMap::new();
        let events = parser.parse(&nf5_packet, test_peer_addr(), &template_cache, &mut seq);

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
        let mut seq = HashMap::new();
        let events = parser.parse(&nf5_packet, test_peer_addr(), &template_cache, &mut seq);

        assert!(!events.is_empty());
        if let Event::Log(log) = &events[0] {
            assert!(log.get("raw_data").is_none());
        }
    }

    #[test]
    fn test_empty_packet() {
        let template_cache = TemplateCache::new(100);
        let config = test_config();
        let parser = ProtocolParser::new(&config, template_cache.clone());
        let mut seq = HashMap::new();
        let result = parse_flow_data(&parser, &[], test_peer_addr(), &template_cache, &mut seq);
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
        let parser = ProtocolParser::new(&config, template_cache.clone());
        let nf5_packet = create_netflow_v5_packet();
        let mut seq = HashMap::new();
        let result = parse_flow_data(
            &parser,
            &nf5_packet,
            test_peer_addr(),
            &template_cache,
            &mut seq,
        );
        assert!(result.is_ok());
        let events = result.unwrap();
        assert!(!events.is_empty());
    }
}
