//! NetFlow v9 protocol parser.
//!
//! NetFlow v9 is a template-based flow export protocol that allows flexible
//! field definitions. Unlike NetFlow v5's fixed format, v9 uses templates
//! to define the structure of flow records, enabling custom fields and
//! variable record formats.

use crate::sources::netflow::events::*;
use crate::sources::netflow::fields::FieldParser;
use crate::sources::netflow::templates::{
    TemplateCache, Template, TemplateField,
    parse_netflow_v9_template_fields,
};
use std::net::SocketAddr;
use vector_lib::event::{Event, LogEvent};


/// NetFlow v9 protocol constants
const NETFLOW_V9_VERSION: u16 = 9;
const NETFLOW_V9_HEADER_SIZE: usize = 20;
const MAX_SET_LENGTH: usize = 65535;
const TEMPLATE_SET_ID: u16 = 0;
const OPTIONS_TEMPLATE_SET_ID: u16 = 1;

/// NetFlow v9 packet header structure
#[derive(Debug, Clone)]
pub struct NetflowV9Header {
    pub version: u16,
    pub count: u16,
    pub sys_uptime: u32,
    pub unix_secs: u32,
    pub flow_sequence: u32,
    pub source_id: u32,
}

impl NetflowV9Header {
    /// Parse NetFlow v9 header from packet data
    pub fn from_bytes(data: &[u8]) -> Result<Self, String> {
        if data.len() < NETFLOW_V9_HEADER_SIZE {
            return Err(format!(
                "Packet too short for NetFlow v9 header: {} bytes, need {}",
                data.len(),
                NETFLOW_V9_HEADER_SIZE
            ));
        }

        let version = u16::from_be_bytes([data[0], data[1]]);
        if version != NETFLOW_V9_VERSION {
            return Err(format!(
                "Invalid NetFlow v9 version: {}, expected {}",
                version, NETFLOW_V9_VERSION
            ));
        }

        Ok(Self {
            version,
            count: u16::from_be_bytes([data[2], data[3]]),
            sys_uptime: u32::from_be_bytes([data[4], data[5], data[6], data[7]]),
            unix_secs: u32::from_be_bytes([data[8], data[9], data[10], data[11]]),
            flow_sequence: u32::from_be_bytes([data[12], data[13], data[14], data[15]]),
            source_id: u32::from_be_bytes([data[16], data[17], data[18], data[19]]),
        })
    }

    /// Create base log event with header information
    pub fn to_log_event(&self) -> LogEvent {
        let mut log_event = LogEvent::default();
        log_event.insert("flow_type", "netflow_v9");
        log_event.insert("version", self.version);
        log_event.insert("count", self.count);
        log_event.insert("sys_uptime", self.sys_uptime);
        log_event.insert("unix_secs", self.unix_secs);
        log_event.insert("flow_sequence", self.flow_sequence);
        log_event.insert("source_id", self.source_id);
        log_event
    }
}

/// NetFlow v9 set header structure
#[derive(Debug, Clone)]
pub struct SetHeader {
    pub set_id: u16,
    pub length: u16,
}

impl SetHeader {
    /// Parse set header from data
    pub fn from_bytes(data: &[u8]) -> Result<Self, String> {
        if data.len() < 4 {
            return Err("Insufficient data for set header".to_string());
        }

        let set_id = u16::from_be_bytes([data[0], data[1]]);
        let length = u16::from_be_bytes([data[2], data[3]]);

        if length < 4 {
            return Err(format!("Invalid set length: {}, minimum is 4", length));
        }

        if length as usize > MAX_SET_LENGTH {
            return Err(format!("Set length too large: {}, maximum is {}", length, MAX_SET_LENGTH));
        }

        Ok(Self { set_id, length })
    }

    /// Check if this is a template set
    pub fn is_template_set(&self) -> bool {
        self.set_id == TEMPLATE_SET_ID
    }

    /// Check if this is an options template set
    pub fn is_options_template_set(&self) -> bool {
        self.set_id == OPTIONS_TEMPLATE_SET_ID
    }

    /// Check if this is a data set
    pub fn is_data_set(&self) -> bool {
        self.set_id >= 256
    }

    /// Get template ID for data sets
    pub fn template_id(&self) -> Option<u16> {
        if self.is_data_set() {
            Some(self.set_id)
        } else {
            None
        }
    }
}

/// NetFlow v9 protocol parser
pub struct NetflowV9Parser {
    field_parser: FieldParser,
}

impl NetflowV9Parser {
    /// Create a new NetFlow v9 parser
    pub fn new(field_parser: FieldParser) -> Self {
        Self {
            field_parser,
        }
    }

    /// Check if packet data looks like NetFlow v9
    pub fn can_parse(data: &[u8]) -> bool {
        if data.len() < NETFLOW_V9_HEADER_SIZE {
            return false;
        }

        let version = u16::from_be_bytes([data[0], data[1]]);
        if version != NETFLOW_V9_VERSION {
            return false;
        }

        // Additional validation - check if count field is reasonable
        let count = u16::from_be_bytes([data[2], data[3]]);
        if count > 1000 {
            // Sanity check - more than 1000 sets seems unreasonable
            return false;
        }

        true
    }

    /// Parse NetFlow v9 packet and return events
    pub fn parse(
        &self,
        data: &[u8],
        peer_addr: SocketAddr,
        template_cache: &TemplateCache,
        include_raw_data: bool,
        drop_unparseable_records: bool,
    ) -> Result<Vec<Event>, String> {
        let mut events = Vec::new();

        // Parse header
        let header = NetflowV9Header::from_bytes(data)?;
        
        debug!(
            "Parsing NetFlow v9 packet: version={}, count={}, source_id={}",
            header.version, header.count, header.source_id
        );

        // Create base event with header info
        let mut base_event = header.to_log_event();
        if include_raw_data {
            let encoded = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, data);
            base_event.insert("raw_data", encoded);
        }

        // Parse sets
        let mut offset = NETFLOW_V9_HEADER_SIZE;
        let mut data_events = Vec::new();
        let mut template_count = 0;
        let mut data_set_count = 0;
        let mut sets_processed = 0;

        while offset + 4 <= data.len() && sets_processed < header.count {
            let set_header = match SetHeader::from_bytes(&data[offset..]) {
                Ok(header) => header,
                Err(e) => {
                    warn!("Invalid set header at offset {}: {}", offset, e);
                    break;
                }
            };

            let set_end = offset + set_header.length as usize;
            if set_end > data.len() {
                warn!(
                    "Set extends beyond packet boundary: offset={}, set_length={}, packet_length={}",
                    offset, set_header.length, data.len()
                );
                break;
            }

            let set_data = &data[offset..set_end];

            match set_header.set_id {
                TEMPLATE_SET_ID => {
                    let parsed_templates = self.parse_template_set(
                        set_data,
                        header.source_id,
                        peer_addr,
                        template_cache,
                    );
                    template_count += parsed_templates;
                }
                OPTIONS_TEMPLATE_SET_ID => {
                    let parsed_templates = self.parse_options_template_set(
                        set_data,
                        header.source_id,
                        peer_addr,
                        template_cache,
                    );
                    template_count += parsed_templates;
                }
                template_id if template_id >= 256 => {
                    let mut set_events = self.parse_data_set(
                        set_data,
                        template_id,
                        header.source_id,
                        peer_addr,
                        template_cache,
                        drop_unparseable_records,
                    );
                    data_set_count += 1;
                    data_events.append(&mut set_events);
                }
                _ => {
                    debug!("Skipping unknown set type: {}", set_header.set_id);
                }
            }

            offset = set_end;
            sets_processed += 1;
        }

        // Add parsed data events
        events.extend(data_events);

        // If no data events were generated, include the header event
        if events.is_empty() && !drop_unparseable_records {
            base_event.insert("template_count", template_count);
            base_event.insert("data_set_count", data_set_count);
            base_event.insert("sets_processed", sets_processed);
            events.push(Event::Log(base_event));
        }

        emit!(NetflowV9PacketProcessed {
            peer_addr,
            template_count,
            data_set_count,
            event_count: events.len(),
            sets_processed: sets_processed as usize,
        });

        Ok(events)
    }

    /// Parse template set and cache templates
    fn parse_template_set(
        &self,
        data: &[u8],
        source_id: u32,
        peer_addr: SocketAddr,
        template_cache: &TemplateCache,
    ) -> usize {
        let mut template_count = 0;
        let mut offset = 4; // Skip set header

        while offset + 4 <= data.len() {
            let template_id = u16::from_be_bytes([data[offset], data[offset + 1]]);
            let field_count = u16::from_be_bytes([data[offset + 2], data[offset + 3]]);

            debug!(
                "Parsing NetFlow v9 template: id={}, fields={}",
                template_id, field_count
            );

            // Validate template ID range (256-65535 for data templates)
            // Note: Options templates can have IDs < 256, so we validate here for regular templates
            if template_id < 256 {
                warn!("Invalid template ID {}, must be >= 256", template_id);
                offset += 4;
                continue;
            }

            // Calculate template end
            let template_end = offset + 4 + (field_count as usize * 4);
            if template_end > data.len() {
                warn!("Template {} extends beyond set boundary", template_id);
                break;
            }

            // Parse template fields
            let template_data = &data[offset..template_end];
            match parse_netflow_v9_template_fields(template_data) {
                Ok(fields) => {
                    // Validate fields
                    if fields.len() != field_count as usize {
                        warn!(
                            "Template {} field count mismatch: expected {}, got {}",
                            template_id, field_count, fields.len()
                        );
                        offset = template_end;
                        continue;
                    }

                    // Check for variable-length fields (not supported in NetFlow v9)
                    let has_variable_fields = fields.iter().any(|f| f.field_length == 0 || f.field_length == 65535);
                    if has_variable_fields {
                        warn!("Template {} contains variable-length fields, which are not supported in NetFlow v9", template_id);
                        offset = template_end;
                        continue;
                    }

                    let template = Template::new(template_id, fields);
                    let key = (peer_addr, source_id, template_id);
                    template_cache.insert(key, template);
                    template_count += 1;

                    emit!(TemplateReceived {
                        template_id,
                        field_count,
                        peer_addr,
                        observation_domain_id: source_id,
                        protocol: "netflow_v9",
                    });
                }
                Err(e) => {
                    emit!(NetflowTemplateError {
                        error: e.as_str(),
                        template_id,
                        peer_addr,
                    });
                }
            }

            offset = template_end;
        }

        template_count
    }

    /// Parse options template set
    fn parse_options_template_set(
        &self,
        data: &[u8],
        source_id: u32,
        peer_addr: SocketAddr,
        template_cache: &TemplateCache,
    ) -> usize {
        let mut template_count = 0;
        let mut offset = 4; // Skip set header

        while offset + 6 <= data.len() {
            let template_id = u16::from_be_bytes([data[offset], data[offset + 1]]);
            let scope_field_count = u16::from_be_bytes([data[offset + 2], data[offset + 3]]);
            let option_field_count = u16::from_be_bytes([data[offset + 4], data[offset + 5]]);

            debug!(
                "Parsing NetFlow v9 options template: id={}, scope_fields={}, option_fields={}",
                template_id, scope_field_count, option_field_count
            );

            let total_fields = scope_field_count + option_field_count;
            let template_end = offset + 6 + (total_fields as usize * 4);
            
            if template_end > data.len() {
                warn!("Options template {} extends beyond set boundary", template_id);
                break;
            }

            // For simplicity, parse options templates like regular templates
            // In a full implementation, scope fields would be handled differently
            let mut fields = Vec::new();
            let mut field_offset = offset + 6;

            for _ in 0..total_fields {
                if field_offset + 4 > data.len() {
                    break;
                }

                let field_type = u16::from_be_bytes([data[field_offset], data[field_offset + 1]]);
                let field_length = u16::from_be_bytes([data[field_offset + 2], data[field_offset + 3]]);

                fields.push(TemplateField {
                    field_type,
                    field_length,
                    enterprise_number: None, // NetFlow v9 doesn't use enterprise numbers
                });

                field_offset += 4;
            }

            if fields.len() == total_fields as usize {
                let template = Template::new(template_id, fields);
                let key = (peer_addr, source_id, template_id);
                template_cache.insert(key, template);
                template_count += 1;

                emit!(TemplateReceived {
                    template_id,
                    field_count: total_fields,
                    peer_addr,
                    observation_domain_id: source_id,
                    protocol: "netflow_v9_options",
                });
            }

            offset = template_end;
        }

        template_count
    }

    /// Parse data set using cached template
    fn parse_data_set(
        &self,
        data: &[u8],
        template_id: u16,
        source_id: u32,
        peer_addr: SocketAddr,
        template_cache: &TemplateCache,
        drop_unparseable_records: bool,
    ) -> Vec<Event> {
        let mut events = Vec::new();
        let key = (peer_addr, source_id, template_id);

        let template = match template_cache.get(&key) {
            Some(template) => template,
            None => {
                debug!(
                    "No template found for NetFlow v9 data set: template_id={}, source_id={}",
                    template_id, source_id
                );

                if drop_unparseable_records {
                    emit!(NetflowEventsDropped {
                        count: 1,
                        reason: "No template available for NetFlow v9 data parsing",
                    });
                    return events;
                }

                // Create basic event without template
                let mut log_event = LogEvent::default();
                log_event.insert("flow_type", "netflow_v9_data_unparseable");
                log_event.insert("template_id", template_id);
                log_event.insert("source_id", source_id);
                log_event.insert("data_length", data.len() - 4); // Exclude set header
                events.push(Event::Log(log_event));
                return events;
            }
        };

        debug!(
            "Parsing NetFlow v9 data set: template_id={}, fields={}",
            template_id,
            template.fields.len()
        );

        // NetFlow v9 only supports fixed-length records
        let record_size = match template.record_size() {
            Some(size) => size,
            None => {
                warn!(
                    "Template {} has variable-length fields, not supported in NetFlow v9",
                    template_id
                );
                return events;
            }
        };

        let mut offset = 4; // Skip set header
        let mut record_count = 0;
        const MAX_RECORDS: usize = 10000; // Safety limit

        while offset + record_size <= data.len() && record_count < MAX_RECORDS {
            let mut log_event = LogEvent::default();
            log_event.insert("flow_type", "netflow_v9_data");
            log_event.insert("template_id", template_id);
            log_event.insert("source_id", source_id);
            log_event.insert("record_number", record_count);

            let mut field_offset = offset;
            let mut fields_parsed = 0;

            for field in &template.fields {
                if field_offset + field.field_length as usize > data.len() {
                    debug!(
                        "Insufficient data for field: offset={}, length={}, remaining={}",
                        field_offset,
                        field.field_length,
                        data.len() - field_offset
                    );
                    break;
                }

                let field_data = &data[field_offset..field_offset + field.field_length as usize];
                self.field_parser.parse_field(field, field_data, &mut log_event);

                field_offset += field.field_length as usize;
                fields_parsed += 1;
            }

            // Only emit event if we parsed all fields successfully
            if fields_parsed == template.fields.len() {
                events.push(Event::Log(log_event));

                emit!(DataRecordParsed {
                    template_id,
                    fields_parsed,
                    record_size,
                    peer_addr,
                    protocol: "netflow_v9",
                });
            } else {
                debug!(
                    "Incomplete record parsing: {}/{} fields parsed",
                    fields_parsed,
                    template.fields.len()
                );
            }

            offset += record_size;
            record_count += 1;
        }

        if record_count >= MAX_RECORDS {
            warn!("Hit maximum record limit ({}) for template {}", MAX_RECORDS, template_id);
        }

        debug!(
            "Parsed {} records from NetFlow v9 data set (template {})",
            record_count, template_id
        );

        events
    }
}

/// NetFlow v9 specific events
#[derive(Debug)]
pub struct NetflowV9PacketProcessed {
    pub peer_addr: SocketAddr,
    pub template_count: usize,
    pub data_set_count: usize,
    pub event_count: usize,
    pub sets_processed: usize,
}

impl vector_lib::internal_event::InternalEvent for NetflowV9PacketProcessed {
    fn emit(self) {
        debug!(
            message = "NetFlow v9 packet processed",
            peer_addr = %self.peer_addr,
            template_count = self.template_count,
            data_set_count = self.data_set_count,
            event_count = self.event_count,
            sets_processed = self.sets_processed,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sources::netflow::config::NetflowConfig;
    use crate::sources::netflow::fields::FieldParser;
    use crate::sources::netflow::templates::TemplateCache;
    use base64::Engine;
    use std::net::{IpAddr, Ipv4Addr};

    fn test_peer_addr() -> SocketAddr {
        SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 100)), 2055)
    }

    fn create_netflow_v9_header() -> Vec<u8> {
        let mut data = vec![0u8; 20];
        data[0..2].copy_from_slice(&9u16.to_be_bytes()); // version
        data[2..4].copy_from_slice(&1u16.to_be_bytes()); // count
        data[4..8].copy_from_slice(&12345u32.to_be_bytes()); // sys_uptime
        data[8..12].copy_from_slice(&1609459200u32.to_be_bytes()); // unix_secs
        data[12..16].copy_from_slice(&100u32.to_be_bytes()); // flow_sequence
        data[16..20].copy_from_slice(&1u32.to_be_bytes()); // source_id
        data
    }

    #[test]
    fn test_netflow_v9_header_parsing() {
        let data = create_netflow_v9_header();
        let header = NetflowV9Header::from_bytes(&data).unwrap();

        assert_eq!(header.version, 9);
        assert_eq!(header.count, 1);
        assert_eq!(header.sys_uptime, 12345);
        assert_eq!(header.unix_secs, 1609459200);
        assert_eq!(header.flow_sequence, 100);
        assert_eq!(header.source_id, 1);
    }

    #[test]
    fn test_invalid_netflow_v9_header() {
        // Too short
        let short_data = vec![0u8; 10];
        assert!(NetflowV9Header::from_bytes(&short_data).is_err());

        // Wrong version
        let mut wrong_version = create_netflow_v9_header();
        wrong_version[0..2].copy_from_slice(&5u16.to_be_bytes());
        assert!(NetflowV9Header::from_bytes(&wrong_version).is_err());
    }

    #[test]
    fn test_can_parse() {
        // Valid NetFlow v9
        let nf9_data = create_netflow_v9_header();
        assert!(NetflowV9Parser::can_parse(&nf9_data));

        // Invalid version
        let mut invalid_data = nf9_data.clone();
        invalid_data[0..2].copy_from_slice(&10u16.to_be_bytes());
        assert!(!NetflowV9Parser::can_parse(&invalid_data));

        // Too short
        let short_data = vec![0u8; 10];
        assert!(!NetflowV9Parser::can_parse(&short_data));

        // Unreasonable count
        let mut bad_count = nf9_data.clone();
        bad_count[2..4].copy_from_slice(&2000u16.to_be_bytes());
        assert!(!NetflowV9Parser::can_parse(&bad_count));
    }

    #[test]
    fn test_set_header_parsing() {
        let mut data = vec![0u8; 8];
        data[0..2].copy_from_slice(&0u16.to_be_bytes()); // template set
        data[2..4].copy_from_slice(&8u16.to_be_bytes()); // length

        let header = SetHeader::from_bytes(&data).unwrap();
        assert_eq!(header.set_id, 0);
        assert_eq!(header.length, 8);
        assert!(header.is_template_set());
        assert!(!header.is_data_set());

        // Data set
        data[0..2].copy_from_slice(&256u16.to_be_bytes());
        let header = SetHeader::from_bytes(&data).unwrap();
        assert!(header.is_data_set());
        assert_eq!(header.template_id(), Some(256));
    }

    #[test]
    fn test_template_parsing() {
        let config = NetflowConfig::default();
        let field_parser = FieldParser::new(&config);
        let template_cache = TemplateCache::new(100);
        let parser = NetflowV9Parser::new(field_parser);

        // Create NetFlow v9 packet with template
        let mut data = create_netflow_v9_header();
        data[2..4].copy_from_slice(&1u16.to_be_bytes()); // count = 1

        // Template set header
        data.extend_from_slice(&0u16.to_be_bytes()); // set_id (template)
        data.extend_from_slice(&12u16.to_be_bytes()); // set_length

        // Template definition
        data.extend_from_slice(&256u16.to_be_bytes()); // template_id
        data.extend_from_slice(&1u16.to_be_bytes()); // field_count
        data.extend_from_slice(&8u16.to_be_bytes()); // field_type (sourceIPv4Address)
        data.extend_from_slice(&4u16.to_be_bytes()); // field_length

        let events = parser.parse(&data, test_peer_addr(), &template_cache, false, false).unwrap();

        // Should have base event with template info
        assert!(!events.is_empty());
        
        // Template should be cached
        let key = (test_peer_addr(), 1, 256);
        assert!(template_cache.get(&key).is_some());
    }

    #[test]
    fn test_data_parsing_with_template() {
        let config = NetflowConfig::default();
        let field_parser = FieldParser::new(&config);
        let template_cache = TemplateCache::new(100);
        let parser = NetflowV9Parser::new(field_parser);

        // First, add a template to cache
        let template = Template::new(
            256,
            vec![TemplateField {
                field_type: 8, // sourceIPv4Address
                field_length: 4,
                enterprise_number: None,
            }],
        );
        let key = (test_peer_addr(), 1, 256);
        template_cache.insert(key, template);

        // Create NetFlow v9 packet with data set
        let mut data = create_netflow_v9_header();

        // Data set header
        data.extend_from_slice(&256u16.to_be_bytes()); // template_id
        data.extend_from_slice(&8u16.to_be_bytes()); // set_length
        data.extend_from_slice(&[192, 168, 1, 1]); // IPv4 address data

        let events = parser.parse(&data, test_peer_addr(), &template_cache, false, false).unwrap();

        // Should parse data using template
        assert!(!events.is_empty());
        if let Event::Log(log) = &events[0] {
            assert_eq!(log.get("flow_type").unwrap().as_str().unwrap(), "netflow_v9_data");
            assert_eq!(log.get("template_id").unwrap().as_integer().unwrap(), 256);
            assert_eq!(log.get("sourceIPv4Address").unwrap().as_str().unwrap(), "192.168.1.1");
        }
    }

    #[test]
    fn test_data_parsing_without_template() {
        let config = NetflowConfig::default();
        let field_parser = FieldParser::new(&config);
        let template_cache = TemplateCache::new(100);
        let parser = NetflowV9Parser::new(field_parser);

        // Create NetFlow v9 packet with data set (no template)
        let mut data = create_netflow_v9_header();

        // Data set header
        data.extend_from_slice(&256u16.to_be_bytes()); // template_id
        data.extend_from_slice(&8u16.to_be_bytes()); // set_length
        data.extend_from_slice(&[0x01, 0x02, 0x03, 0x04]); // Some data

        let events = parser.parse(&data, test_peer_addr(), &template_cache, false, false).unwrap();

        // Should create unparseable event
        assert!(!events.is_empty());
        if let Event::Log(log) = &events[0] {
            assert!(log.get("flow_type").unwrap().as_str().unwrap().contains("unparseable"));
        }
    }

    #[test]
    fn test_options_template_parsing() {
        let config = NetflowConfig::default();
        let field_parser = FieldParser::new(&config);
        let template_cache = TemplateCache::new(100);
        let parser = NetflowV9Parser::new(field_parser);

        // Create NetFlow v9 packet with options template
        let mut data = create_netflow_v9_header();
        data[2..4].copy_from_slice(&1u16.to_be_bytes()); // count = 1 set

        // Options template set header
        data.extend_from_slice(&1u16.to_be_bytes()); // set_id (options template)
        data.extend_from_slice(&14u16.to_be_bytes()); // set_length

        // Options template definition
        data.extend_from_slice(&257u16.to_be_bytes()); // template_id
        data.extend_from_slice(&1u16.to_be_bytes()); // scope_field_count
        data.extend_from_slice(&1u16.to_be_bytes()); // option_field_count
        data.extend_from_slice(&1u16.to_be_bytes()); // scope field type
        data.extend_from_slice(&4u16.to_be_bytes()); // scope field length
        data.extend_from_slice(&2u16.to_be_bytes()); // option field type
        data.extend_from_slice(&4u16.to_be_bytes()); // option field length
        let events = parser.parse(&data, test_peer_addr(), &template_cache, false, false).unwrap();

        // Should parse options template
        assert!(!events.is_empty());
        
        // Debug: print all cached templates
        let debug_templates = template_cache.debug_templates(10);
        println!("Cached templates: {:?}", debug_templates);
        println!("Template cache stats: {:?}", template_cache.stats());
        
        // Template should be cached - check with correct key
        let key = (test_peer_addr(), 1, 257);
        let template = template_cache.get(&key);
        assert!(template.is_some(), "Template should be cached for key {:?}", key);
    }

   #[test]
   fn test_multiple_records_in_data_set() {
       let config = NetflowConfig::default();
       let field_parser = FieldParser::new(&config);
       let template_cache = TemplateCache::new(100);
       let parser = NetflowV9Parser::new(field_parser);

       // Add template with fixed-length field
       let template = Template::new(
           256,
           vec![TemplateField {
               field_type: 8, // sourceIPv4Address
               field_length: 4,
               enterprise_number: None,
           }],
       );
       let key = (test_peer_addr(), 1, 256);
       template_cache.insert(key, template);

       // Create NetFlow v9 packet with multiple records
       let mut data = create_netflow_v9_header();

       // Data set header
       data.extend_from_slice(&256u16.to_be_bytes()); // template_id
       data.extend_from_slice(&12u16.to_be_bytes()); // set_length (4 header + 8 data)

       // Two IPv4 records
       data.extend_from_slice(&[192, 168, 1, 1]); // First record
       data.extend_from_slice(&[10, 0, 0, 1]);    // Second record

       let events = parser.parse(&data, test_peer_addr(), &template_cache, false, false).unwrap();

       // Should get two data events
       assert_eq!(events.len(), 2);
       
       if let Event::Log(log1) = &events[0] {
           assert_eq!(log1.get("sourceIPv4Address").unwrap().as_str().unwrap(), "192.168.1.1");
           assert_eq!(log1.get("record_number").unwrap().as_integer().unwrap(), 0);
       }
       
       if let Event::Log(log2) = &events[1] {
           assert_eq!(log2.get("sourceIPv4Address").unwrap().as_str().unwrap(), "10.0.0.1");
           assert_eq!(log2.get("record_number").unwrap().as_integer().unwrap(), 1);
       }
   }

   #[test]
   fn test_mixed_template_and_data_packet() {
       let config = NetflowConfig::default();
       let field_parser = FieldParser::new(&config);
       let template_cache = TemplateCache::new(100);
       let parser = NetflowV9Parser::new(field_parser);

       // Create NetFlow v9 packet with both template and data
       let mut data = create_netflow_v9_header();
       data[2..4].copy_from_slice(&2u16.to_be_bytes()); // count = 2 sets

       // Template set
       data.extend_from_slice(&0u16.to_be_bytes()); // set_id (template)
       data.extend_from_slice(&12u16.to_be_bytes()); // set_length
       data.extend_from_slice(&256u16.to_be_bytes()); // template_id
       data.extend_from_slice(&1u16.to_be_bytes()); // field_count
       data.extend_from_slice(&8u16.to_be_bytes()); // field_type
       data.extend_from_slice(&4u16.to_be_bytes()); // field_length

       // Data set
       data.extend_from_slice(&256u16.to_be_bytes()); // template_id
       data.extend_from_slice(&8u16.to_be_bytes()); // set_length
       data.extend_from_slice(&[192, 168, 1, 1]); // IPv4 data

       let events = parser.parse(&data, test_peer_addr(), &template_cache, false, false).unwrap();

       // Should get data event (template was cached and immediately used)
       assert!(!events.is_empty());
       
       // Should have cached the template
       let key = (test_peer_addr(), 1, 256);
       assert!(template_cache.get(&key).is_some());
       
       // Should have at least one data event
       let data_events: Vec<_> = events.iter()
           .filter(|e| {
               if let Event::Log(log) = e {
                   log.get("flow_type").unwrap().as_str().unwrap() == "netflow_v9_data"
               } else {
                   false
               }
           })
           .collect();
       assert!(!data_events.is_empty());
   }

   #[test]
   fn test_invalid_template_id() {
       let config = NetflowConfig::default();
       let field_parser = FieldParser::new(&config);
       let template_cache = TemplateCache::new(100);
       let parser = NetflowV9Parser::new(field_parser);

       // Create NetFlow v9 packet with invalid template ID
       let mut data = create_netflow_v9_header();

       // Template set with invalid template ID
       data.extend_from_slice(&0u16.to_be_bytes()); // set_id (template)
       data.extend_from_slice(&12u16.to_be_bytes()); // set_length
       data.extend_from_slice(&100u16.to_be_bytes()); // invalid template_id (< 256)
       data.extend_from_slice(&1u16.to_be_bytes()); // field_count
       data.extend_from_slice(&8u16.to_be_bytes()); // field_type
       data.extend_from_slice(&4u16.to_be_bytes()); // field_length

       let events = parser.parse(&data, test_peer_addr(), &template_cache, false, false).unwrap();

       // Should handle gracefully
       assert!(!events.is_empty());
       
       // Template should not be cached due to invalid ID
       let key = (test_peer_addr(), 1, 100);
       assert!(template_cache.get(&key).is_none());
   }

   #[test]
   fn test_template_field_count_mismatch() {
       let config = NetflowConfig::default();
       let field_parser = FieldParser::new(&config);
       let template_cache = TemplateCache::new(100);
       let parser = NetflowV9Parser::new(field_parser);

       // Create NetFlow v9 packet with mismatched field count
       let mut data = create_netflow_v9_header();

       // Template set with wrong field count
       data.extend_from_slice(&0u16.to_be_bytes()); // set_id (template)
       data.extend_from_slice(&12u16.to_be_bytes()); // set_length
       data.extend_from_slice(&256u16.to_be_bytes()); // template_id
       data.extend_from_slice(&2u16.to_be_bytes()); // field_count (says 2)
       data.extend_from_slice(&8u16.to_be_bytes()); // field_type
       data.extend_from_slice(&4u16.to_be_bytes()); // field_length
       // Missing second field

       let events = parser.parse(&data, test_peer_addr(), &template_cache, false, false).unwrap();

       // Should handle gracefully
       assert!(!events.is_empty());
       
       // Template should not be cached due to mismatch
       let key = (test_peer_addr(), 1, 256);
       assert!(template_cache.get(&key).is_none());
   }

   #[test]
   fn test_variable_length_field_rejection() {
       let config = NetflowConfig::default();
       let field_parser = FieldParser::new(&config);
       let template_cache = TemplateCache::new(100);
       let parser = NetflowV9Parser::new(field_parser);

       // Create template with variable-length field
       let template = Template::new(
           256,
           vec![TemplateField {
               field_type: 8,
               field_length: 65535, // Variable length
               enterprise_number: None,
           }],
       );
       let key = (test_peer_addr(), 1, 256);
       template_cache.insert(key.clone(), template);

       // Create NetFlow v9 packet with data set
       let mut data = create_netflow_v9_header();

       // Data set header
       data.extend_from_slice(&256u16.to_be_bytes()); // template_id
       data.extend_from_slice(&8u16.to_be_bytes()); // set_length
       data.extend_from_slice(&[0x01, 0x02, 0x03, 0x04]);

       let events = parser.parse(&data, test_peer_addr(), &template_cache, false, false).unwrap();

       // Should reject variable-length fields in NetFlow v9
       // Events will be empty or contain error info
       if !events.is_empty() {
           // If events are generated, they should not be successful data events
           for event in &events {
               if let Event::Log(log) = event {
                   let flow_type = log.get("flow_type").unwrap().as_str().unwrap();
                   assert_ne!(flow_type, "netflow_v9_data");
               }
           }
       }
   }

   #[test]
   fn test_drop_unparseable_records() {
       let config = NetflowConfig::default();
       let field_parser = FieldParser::new(&config);
       let template_cache = TemplateCache::new(100);
       let parser = NetflowV9Parser::new(field_parser);

       // Create NetFlow v9 packet with data set (no template)
       let mut data = create_netflow_v9_header();

       // Data set header
       data.extend_from_slice(&256u16.to_be_bytes()); // template_id
       data.extend_from_slice(&8u16.to_be_bytes()); // set_length
       data.extend_from_slice(&[0x01, 0x02, 0x03, 0x04]);

       // With drop_unparseable_records = true, should get no events
       let events = parser.parse(&data, test_peer_addr(), &template_cache, false, true).unwrap();
       assert!(events.is_empty());

       // With drop_unparseable_records = false, should get unparseable event
       let events = parser.parse(&data, test_peer_addr(), &template_cache, false, false).unwrap();
       assert!(!events.is_empty());
   }

   #[test]
   fn test_malformed_packet_handling() {
       let config = NetflowConfig::default();
       let field_parser = FieldParser::new(&config);
       let template_cache = TemplateCache::new(100);
       let parser = NetflowV9Parser::new(field_parser);

       // Test with corrupted set length
       let mut data = create_netflow_v9_header();

       // Corrupted set header
       data.extend_from_slice(&0u16.to_be_bytes()); // set_id
       data.extend_from_slice(&100u16.to_be_bytes()); // Invalid large length

       let result = parser.parse(&data, test_peer_addr(), &template_cache, false, false);
       
       // Should handle gracefully and return base event
       assert!(result.is_ok());
       let events = result.unwrap();
       assert!(!events.is_empty());
   }

   #[test]
   fn test_raw_data_inclusion() {
       let config = NetflowConfig::default();
       let field_parser = FieldParser::new(&config);
       let template_cache = TemplateCache::new(100);
       let parser = NetflowV9Parser::new(field_parser);

       let data = create_netflow_v9_header();

       // Test with raw data inclusion
       let events = parser.parse(&data, test_peer_addr(), &template_cache, true, false).unwrap();
       assert!(!events.is_empty());
       
       if let Event::Log(log) = &events[0] {
           assert!(log.get("raw_data").is_some());
           let raw_data = log.get("raw_data").unwrap().as_str().unwrap();
           
           // Should be valid base64
           assert!(base64::engine::general_purpose::STANDARD.decode(raw_data.as_bytes()).is_ok());
       }

       // Test without raw data inclusion
       let events = parser.parse(&data, test_peer_addr(), &template_cache, false, false).unwrap();
       assert!(!events.is_empty());
       
       if let Event::Log(log) = &events[0] {
           assert!(log.get("raw_data").is_none());
       }
   }

   #[test]
   fn test_record_safety_limits() {
       let config = NetflowConfig::default();
       let field_parser = FieldParser::new(&config);
       let template_cache = TemplateCache::new(100);
       let parser = NetflowV9Parser::new(field_parser);

       // Add template with small field to potentially create many records
       let template = Template::new(
           256,
           vec![TemplateField {
               field_type: 4, // protocolIdentifier
               field_length: 1,
               enterprise_number: None,
           }],
       );
       let key = (test_peer_addr(), 1, 256);
       template_cache.insert(key, template);

       // Create NetFlow v9 packet with large data set
       let mut data = create_netflow_v9_header();
       let data_size = 10000; // Large data set

       // Data set header
       data.extend_from_slice(&256u16.to_be_bytes()); // template_id
       data.extend_from_slice(&((4 + data_size) as u16).to_be_bytes()); // set_length

       // Add lots of data (each record is 1 byte)
       data.extend(vec![6u8; data_size]); // All TCP protocol

       let events = parser.parse(&data, test_peer_addr(), &template_cache, false, false).unwrap();

       // Should be limited by MAX_RECORDS safety limit
       assert!(events.len() <= 10000); // MAX_RECORDS constant
       
       // All events should be valid
       for event in &events {
           if let Event::Log(log) = event {
               assert_eq!(log.get("flow_type").unwrap().as_str().unwrap(), "netflow_v9_data");
               assert_eq!(log.get("protocolIdentifier").unwrap().as_integer().unwrap(), 6);
           }
       }
   }

   #[test]
   fn test_header_to_log_event() {
       let data = create_netflow_v9_header();
       let header = NetflowV9Header::from_bytes(&data).unwrap();
       let log_event = header.to_log_event();

       assert_eq!(log_event.get("flow_type").unwrap().as_str().unwrap(), "netflow_v9");
       assert_eq!(log_event.get("version").unwrap().as_integer().unwrap(), 9);
       assert_eq!(log_event.get("count").unwrap().as_integer().unwrap(), 1);
       assert_eq!(log_event.get("sys_uptime").unwrap().as_integer().unwrap(), 12345);
       assert_eq!(log_event.get("unix_secs").unwrap().as_integer().unwrap(), 1609459200);
       assert_eq!(log_event.get("flow_sequence").unwrap().as_integer().unwrap(), 100);
       assert_eq!(log_event.get("source_id").unwrap().as_integer().unwrap(), 1);
   }

   #[test]
   fn test_incomplete_record_handling() {
       let config = NetflowConfig::default();
       let field_parser = FieldParser::new(&config);
       let template_cache = TemplateCache::new(100);
       let parser = NetflowV9Parser::new(field_parser);

       // Add template with 8-byte record
       let template = Template::new(
           256,
           vec![
               TemplateField {
                   field_type: 8, // sourceIPv4Address
                   field_length: 4,
                   enterprise_number: None,
               },
               TemplateField {
                   field_type: 12, // destinationIPv4Address
                   field_length: 4,
                   enterprise_number: None,
               },
           ],
       );
       let key = (test_peer_addr(), 1, 256);
       template_cache.insert(key, template);

       // Create NetFlow v9 packet with incomplete record
       let mut data = create_netflow_v9_header();

       // Data set header
       data.extend_from_slice(&256u16.to_be_bytes()); // template_id
       data.extend_from_slice(&10u16.to_be_bytes()); // set_length

       // Incomplete record (only 6 bytes instead of 8)
       data.extend_from_slice(&[192, 168, 1, 1, 10, 0]); // Missing 2 bytes

       let events = parser.parse(&data, test_peer_addr(), &template_cache, false, false).unwrap();

       // Should handle incomplete records gracefully
       // Either no events or events that don't include the incomplete record
       for event in &events {
           if let Event::Log(log) = event {
               let flow_type = log.get("flow_type").unwrap().as_str().unwrap();
               if flow_type == "netflow_v9_data" {
                   // If a data event was created, it should have both fields
                   assert!(log.get("sourceIPv4Address").is_some());
                   assert!(log.get("destinationIPv4Address").is_some());
               }
           }
       }
   }
}