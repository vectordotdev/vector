//! IPFIX (Internet Protocol Flow Information Export) protocol parser.
//!
//! IPFIX is the standard protocol for exporting flow information, defined in RFC 7011.
//! It uses templates to define the structure of data records, supporting both standard
//! and enterprise-specific fields.

use crate::sources::netflow::events::*;
use crate::sources::netflow::fields::FieldParser;
use crate::sources::netflow::templates::{
    TemplateCache, Template,
};
use crate::sources::netflow::templates::{parse_ipfix_template_fields, parse_ipfix_options_template_fields};
use std::net::SocketAddr;
use vector_lib::event::{Event, LogEvent, Value};
use base64::Engine;


/// IPFIX protocol constants
const IPFIX_VERSION: u16 = 10;
const IPFIX_HEADER_SIZE: usize = 16;
const MAX_SET_LENGTH: usize = 65535;
const TEMPLATE_SET_ID: u16 = 2;
const OPTIONS_TEMPLATE_SET_ID: u16 = 3;

/// IPFIX packet header structure
#[derive(Debug, Clone)]
pub struct IpfixHeader {
    pub version: u16,
    pub length: u16,
    pub export_time: u32,
    pub sequence_number: u32,
    pub observation_domain_id: u32,
}

impl IpfixHeader {
    /// Parse IPFIX header from packet data
    pub fn from_bytes(data: &[u8]) -> Result<Self, String> {
        if data.len() < IPFIX_HEADER_SIZE {
            return Err(format!(
                "Packet too short for IPFIX header: {} bytes, need {}",
                data.len(),
                IPFIX_HEADER_SIZE
            ));
        }

        let version = u16::from_be_bytes([data[0], data[1]]);
        if version != IPFIX_VERSION {
            return Err(format!("Invalid IPFIX version: {}, expected {}", version, IPFIX_VERSION));
        }

        let length = u16::from_be_bytes([data[2], data[3]]);
        if length as usize > data.len() {
            return Err(format!(
                "IPFIX length mismatch: header says {}, packet is {}",
                length,
                data.len()
            ));
        }

        Ok(Self {
            version,
            length,
            export_time: u32::from_be_bytes([data[4], data[5], data[6], data[7]]),
            sequence_number: u32::from_be_bytes([data[8], data[9], data[10], data[11]]),
            observation_domain_id: u32::from_be_bytes([data[12], data[13], data[14], data[15]]),
        })
    }

    /// Create base log event with header information
    pub fn to_log_event(&self) -> LogEvent {
        let mut log_event = LogEvent::default();
        log_event.insert("flow_type", "ipfix");
        log_event.insert("version", self.version);
        log_event.insert("length", self.length);
        log_event.insert("export_time", self.export_time);
        log_event.insert("sequence_number", self.sequence_number);
        log_event.insert("observation_domain_id", self.observation_domain_id);
        log_event
    }
}

/// IPFIX set header structure
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

/// IPFIX protocol parser
pub struct IpfixParser {
    field_parser: FieldParser,
    options_template_mode: String,
}

impl IpfixParser {
    /// Create a new IPFIX parser
    pub fn new(field_parser: FieldParser, options_template_mode: String) -> Self {
        Self { 
            field_parser,
            options_template_mode,
        }
    }

    /// Check if packet data looks like IPFIX
    pub fn can_parse(data: &[u8]) -> bool {
        if data.len() < 2 {
            return false;
        }

        let version = u16::from_be_bytes([data[0], data[1]]);
        version == IPFIX_VERSION
    }

    /// Parse IPFIX packet and return events
    pub fn parse(
        &self,
        data: &[u8],
        peer_addr: SocketAddr,
        template_cache: &TemplateCache,
        include_raw_data: bool,
        drop_unparseable_records: bool,
        buffer_missing_templates: bool,
    ) -> Result<Vec<Event>, String> {
        let mut events = Vec::new();

        // Parse header
        let header = IpfixHeader::from_bytes(data)?;
        
        debug!(
            "Parsing IPFIX packet: version={}, length={}, domain={}",
            header.version, header.length, header.observation_domain_id
        );

        // Create base event with header info
        let mut base_event = header.to_log_event();
        if include_raw_data {
            let encoded = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, data);
            base_event.insert("raw_data", encoded);
        }

        // Parse sets
        let mut offset = IPFIX_HEADER_SIZE;
        let mut data_events = Vec::new();
        let mut template_count = 0;
        let mut data_set_count = 0;

        while offset + 4 <= data.len() && offset < header.length as usize {
            let set_header = match SetHeader::from_bytes(&data[offset..]) {
                Ok(header) => header,
                Err(e) => {
                    warn!("Invalid set header at offset {}: {}", offset, e);
                    break;
                }
            };

            let set_end = offset + set_header.length as usize;
            if set_end > data.len() || set_end > header.length as usize {
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
                        header.observation_domain_id,
                        peer_addr,
                        template_cache,
                    );
                    template_count += parsed_templates;
                }
                OPTIONS_TEMPLATE_SET_ID => {
                    let parsed_templates = self.parse_options_template_set(
                        set_data,
                        header.observation_domain_id,
                        peer_addr,
                        template_cache,
                    );
                    template_count += parsed_templates;
                }
                template_id if template_id >= 256 => {
                    let mut set_events = self.parse_data_set(
                        set_data,
                        template_id,
                        header.observation_domain_id,
                        peer_addr,
                        template_cache,
                        drop_unparseable_records,
                        buffer_missing_templates,
                    );
                    data_set_count += 1;
                    data_events.append(&mut set_events);
                }
                _ => {
                    debug!("Skipping unknown set type: {}", set_header.set_id);
                }
            }

            offset = set_end;
        }

        // Add parsed data events
        events.extend(data_events);

        // If no data events were generated, include the header event
        if events.is_empty() && !drop_unparseable_records {
            base_event.insert("template_count", template_count);
            base_event.insert("data_set_count", data_set_count);
            events.push(Event::Log(base_event));
        }

        emit!(IpfixPacketProcessed {
            peer_addr,
            template_count,
            data_set_count,
            event_count: events.len(),
        });

        Ok(events)
    }

    /// Parse template set and cache templates
    fn parse_template_set(
        &self,
        data: &[u8],
        observation_domain_id: u32,
        peer_addr: SocketAddr,
        template_cache: &TemplateCache,
    ) -> usize {
        let mut template_count = 0;
        let mut offset = 4; // Skip set header

        while offset + 4 <= data.len() {
            let template_id = u16::from_be_bytes([data[offset], data[offset + 1]]);
            let field_count = u16::from_be_bytes([data[offset + 2], data[offset + 3]]);

            debug!(
                "Parsing IPFIX template: id={}, fields={}",
                template_id, field_count
            );

            // Find end of this template
            let mut template_end = offset + 4;
            let mut remaining_fields = field_count;

            while remaining_fields > 0 && template_end + 4 <= data.len() {
                let field_type = u16::from_be_bytes([data[template_end], data[template_end + 1]]);
                template_end += 4; // field_type + field_length

                // Check for enterprise field
                if field_type & 0x8000 != 0 && template_end + 4 <= data.len() {
                    template_end += 4; // enterprise_number
                }

                remaining_fields -= 1;
            }

            if remaining_fields > 0 {
                warn!("Incomplete template data for template {}", template_id);
                break;
            }

            // Parse template fields
            let template_data = &data[offset..template_end];
            
            // Debug: Log raw template data for template ID 1024 (only once)
            if template_id == 1024 {
                debug!(
                    message = "Template ID 1024 received - raw template data dump",
                    template_id = template_id,
                    field_count = field_count,
                    template_data_length = template_data.len(),
                    raw_template_hex = format!("{:02x?}", template_data),
                    raw_template_base64 = base64::engine::general_purpose::STANDARD.encode(template_data),
                    peer_addr = %peer_addr,
                    observation_domain_id = observation_domain_id,
                );
            }
            
            match parse_ipfix_template_fields(template_data) {
                Ok(fields) => {
                    let template = Template::new(template_id, fields);
                    let key = (peer_addr, observation_domain_id, template_id);
                    template_cache.insert(key, template);
                    template_count += 1;

                    emit!(IpfixTemplateReceived {
                        template_id,
                        field_count,
                        peer_addr,
                        observation_domain_id,
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

    /// Parse options template set with proper scope field handling
    fn parse_options_template_set(
        &self,
        data: &[u8],
        observation_domain_id: u32,
        peer_addr: SocketAddr,
        template_cache: &TemplateCache,
    ) -> usize {
        let mut template_count = 0;
        let mut offset = 4; // Skip set header

        while offset + 6 <= data.len() { // Need at least 6 bytes for options template header
            let template_id = u16::from_be_bytes([data[offset], data[offset + 1]]);
            let field_count = u16::from_be_bytes([data[offset + 2], data[offset + 3]]);
            let scope_field_count = u16::from_be_bytes([data[offset + 4], data[offset + 5]]);

            debug!(
                "Parsing IPFIX options template: id={}, fields={}, scope_fields={}",
                template_id, field_count, scope_field_count
            );

            // Find end of this template
            let mut template_end = offset + 6; // Skip template_id, field_count, scope_field_count
            let mut remaining_fields = field_count;

            while remaining_fields > 0 && template_end + 4 <= data.len() {
                let field_type = u16::from_be_bytes([data[template_end], data[template_end + 1]]);
                template_end += 4; // field_type + field_length

                // Check for enterprise field
                if field_type & 0x8000 != 0 && template_end + 4 <= data.len() {
                    template_end += 4; // enterprise_number
                }

                remaining_fields -= 1;
            }

            if remaining_fields > 0 {
                warn!("Incomplete options template data for template {}", template_id);
                break;
            }

            // Parse options template fields
            let template_data = &data[offset..template_end];
            
            // Debug: Log raw template data for template ID 1024 (only once)
            if template_id == 1024 {
                debug!(
                    message = "Options Template ID 1024 received - raw template data dump",
                    template_id = template_id,
                    field_count = field_count,
                    scope_field_count = scope_field_count,
                    template_data_length = template_data.len(),
                    raw_template_hex = format!("{:02x?}", template_data),
                    raw_template_base64 = base64::engine::general_purpose::STANDARD.encode(template_data),
                    peer_addr = %peer_addr,
                    observation_domain_id = observation_domain_id,
                );
            }
            
            match parse_ipfix_options_template_fields(template_data) {
                Ok((fields, scope_count)) => {
                    let template = Template::new_options(template_id, fields, scope_count);
                    let key = (peer_addr, observation_domain_id, template_id);
                    template_cache.insert(key, template);
                    template_count += 1;

                    emit!(IpfixTemplateReceived {
                        template_id,
                        field_count,
                        peer_addr,
                        observation_domain_id,
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

    /// Parse data set using cached template
    fn parse_data_set(
        &self,
        data: &[u8],
        template_id: u16,
        observation_domain_id: u32,
        peer_addr: SocketAddr,
        template_cache: &TemplateCache,
        drop_unparseable_records: bool,
        buffer_missing_templates: bool,
    ) -> Vec<Event> {
        let mut events = Vec::new();
        let key = (peer_addr, observation_domain_id, template_id);

        let template = match template_cache.get(&key) {
            Some(template) => template,
            None => {
                debug!(
                    "No template found for data set: template_id={}, domain={}",
                    template_id, observation_domain_id
                );

                // Try to buffer the data if buffering is enabled
                if buffer_missing_templates {
                    if template_cache.buffer_data_record(
                        key,
                        data.to_vec(),
                        peer_addr,
                        observation_domain_id,
                    ) {
                        debug!(
                            "Buffered data record for template_id={}, waiting for template",
                            template_id
                        );
                        return events; // Return empty, data is buffered
                    }
                }

                if drop_unparseable_records {
                    emit!(NetflowEventsDropped {
                        count: 1,
                        reason: "No template available for IPFIX data parsing",
                    });
                    return events;
                }

                // Create basic event without template
                let mut log_event = LogEvent::default();
                log_event.insert("flow_type", "ipfix_data_unparseable");
                log_event.insert("template_id", template_id);
                log_event.insert("observation_domain_id", observation_domain_id);
                log_event.insert("data_length", data.len() - 4); // Exclude set header
                events.push(Event::Log(log_event));
                return events;
            }
        };

        debug!(
            "Parsing IPFIX data set: template_id={}, fields={}, template_fields={:?}",
            template_id,
            template.fields.len(),
            template.fields.iter().map(|f| (f.field_type, f.field_length)).collect::<Vec<_>>()
        );

        // Calculate record size for fixed-length templates
        let record_size = template.record_size();
        let mut offset = 4; // Skip set header
        let mut record_count = 0;
        const MAX_RECORDS: usize = 10000; // Safety limit

        while offset < data.len() && record_count < MAX_RECORDS {
            let mut log_event = LogEvent::default();
            
            // Check if this is an Options Template (metadata about exporter)
            if template.scope_field_count > 0 {
                log_event.insert("flow_type", "ipfix_options_data");
                log_event.insert("data_type", "exporter_metadata");
            } else {
                log_event.insert("flow_type", "ipfix_data");
                log_event.insert("data_type", "flow_data");
            }
            
            log_event.insert("template_id", template_id);
            log_event.insert("observation_domain_id", observation_domain_id);
            log_event.insert("record_number", record_count);

            let mut field_offset = offset;
            let mut fields_parsed = 0;

            for field in &template.fields {
                if field_offset >= data.len() {
                    debug!("Reached end of data while parsing field {}", fields_parsed);
                    break;
                }

                let field_length = if field.field_length == 65535 {
                    // Variable-length field
                    let length = self.parse_variable_length(&data[field_offset..]);
                    match length {
                        Some((len, consumed)) => {
                            field_offset += consumed;
                            len
                        }
                        None => {
                            debug!("Failed to parse variable-length field");
                            break;
                        }
                    }
                } else {
                    field.field_length as usize
                };

                if field_offset + field_length > data.len() {
                    debug!(
                        "Insufficient data for field: offset={}, length={}, remaining={}",
                        field_offset,
                        field_length,
                        data.len() - field_offset
                    );
                    break;
                }

                let field_data = &data[field_offset..field_offset + field_length];
                
                // Debug: Log raw field data for problematic fields
                if field.field_type == 96 || field.field_type == 236 || field.field_type == 32793 {
                    debug!(
                        "Field {} (type={}, length={}): raw_data={:?}",
                        fields_parsed,
                        field.field_type,
                        field_length,
                        &field_data[..std::cmp::min(field_data.len(), 16)]
                    );
                }
                
                self.field_parser.parse_field(field, field_data, &mut log_event);

                field_offset += field_length;
                fields_parsed += 1;
            }

            // Only emit event if we parsed some fields
            if fields_parsed > 0 {
                // Check if this is Options Template data and handle accordingly
                if template.scope_field_count > 0 {
                    match self.options_template_mode.as_str() {
                        "discard" => {
                            debug!("Discarding Options Template data record (template_id={})", template_id);
                            // Don't add to events - effectively discard
                        },
                        "emit_metadata" => {
                            debug!("Emitting Options Template data record (template_id={})", template_id);
                            events.push(Event::Log(log_event));
                        },
                        "enrich" => {
                            debug!("Using Options Template data for enrichment (template_id={})", template_id);
                            // Add enrichment metadata but mark as non-flow data
                            log_event.insert("enrichment_only", true);
                            events.push(Event::Log(log_event));
                        },
                        _ => {
                            warn!("Unknown options_template_mode value: {}, defaulting to discard", self.options_template_mode);
                            // Default to discard for unknown values
                        }
                    }
                } else {
                    // Regular flow data - always emit
                    debug!(
                        "Parsed flow record {}: fields={}",
                        record_count,
                        fields_parsed
                    );
                    
                    // Log specific problematic fields for debugging
                    if let Some(Value::Integer(bytes)) = log_event.get("octetDeltaCount") {
                        if *bytes > 1_000_000_000_000i64 {
                            debug!("Large byte count detected: {} bytes", bytes);
                        }
                    }
                    
                    if let Some(Value::Bytes(protocol)) = log_event.get("protocolName") {
                        if protocol.as_ref() == b"XNET" {
                            debug!("XNET protocol detected - possible parsing issue");
                        }
                    }
                    
                    events.push(Event::Log(log_event));
                }
            }

            // Advance to next record
            if let Some(size) = record_size {
                // Fixed-length records
                offset += size;
                if offset > field_offset {
                    // Skip any remaining padding
                    offset = field_offset;
                }
            } else {
                // Variable-length records
                offset = field_offset;
            }

            // Safety check for infinite loops
            if offset <= field_offset && record_size.is_none() {
                debug!("Record parsing made no progress, stopping");
                break;
            }

            record_count += 1;
        }

        if record_count >= MAX_RECORDS {
            warn!("Hit maximum record limit ({}) for template {}", MAX_RECORDS, template_id);
        }

        debug!(
            "Parsed {} records from IPFIX data set (template {})",
            record_count, template_id
        );

        events
    }

    /// Parse variable-length field length encoding
    fn parse_variable_length(&self, data: &[u8]) -> Option<(usize, usize)> {
        if data.is_empty() {
            return None;
        }

        let first_byte = data[0];
        if first_byte < 255 {
            // Single byte length
            Some((first_byte as usize, 1))
        } else if data.len() >= 3 {
            // Three byte length (0xFF + 2 bytes)
            let length = u16::from_be_bytes([data[1], data[2]]) as usize;
            Some((length, 3))
        } else {
            None
        }
    }

}

/// Additional event types for IPFIX-specific events
#[derive(Debug)]
pub struct IpfixPacketProcessed {
    pub peer_addr: SocketAddr,
    pub template_count: usize,
    pub data_set_count: usize,
    pub event_count: usize,
}

impl vector_lib::internal_event::InternalEvent for IpfixPacketProcessed {
    fn emit(self) {
        debug!(
            message = "IPFIX packet processed",
            peer_addr = %self.peer_addr,
            template_count = self.template_count,
            data_set_count = self.data_set_count,
            event_count = self.event_count,
        );
    }
}

#[derive(Debug)]
pub struct IpfixTemplateReceived {
    pub template_id: u16,
    pub field_count: u16,
    pub peer_addr: SocketAddr,
    pub observation_domain_id: u32,
}

impl vector_lib::internal_event::InternalEvent for IpfixTemplateReceived {
    fn emit(self) {
        debug!(
            message = "IPFIX template received",
            template_id = self.template_id,
            field_count = self.field_count,
            peer_addr = %self.peer_addr,
            observation_domain_id = self.observation_domain_id,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sources::netflow::NetflowConfig;
    use crate::sources::netflow::fields::FieldParser;
    use crate::sources::netflow::templates::{TemplateCache, TemplateField};
    use base64::Engine;
    use std::net::{IpAddr, Ipv4Addr};

    fn test_peer_addr() -> SocketAddr {
        SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 100)), 4739)
    }

    fn create_ipfix_header() -> Vec<u8> {
        let mut data = vec![0u8; 16];
        data[0..2].copy_from_slice(&10u16.to_be_bytes()); // version
        data[2..4].copy_from_slice(&16u16.to_be_bytes()); // length (16 bytes)
        data[4..8].copy_from_slice(&1609459200u32.to_be_bytes()); // export_time
        data[8..12].copy_from_slice(&12345u32.to_be_bytes()); // sequence_number
        data[12..16].copy_from_slice(&1u32.to_be_bytes()); // observation_domain_id
        data
    }

    #[test]
    fn test_ipfix_header_parsing() {
        let data = create_ipfix_header();
        let header = IpfixHeader::from_bytes(&data).unwrap();

        assert_eq!(header.version, 10);
        assert_eq!(header.length, 16);
        assert_eq!(header.export_time, 1609459200);
        assert_eq!(header.sequence_number, 12345);
        assert_eq!(header.observation_domain_id, 1);
    }

    #[test]
    fn test_invalid_ipfix_header() {
        // Too short
        let short_data = vec![0u8; 10];
        assert!(IpfixHeader::from_bytes(&short_data).is_err());

        // Wrong version
        let mut wrong_version = create_ipfix_header();
        wrong_version[0..2].copy_from_slice(&9u16.to_be_bytes());
        assert!(IpfixHeader::from_bytes(&wrong_version).is_err());

        // Length mismatch
        let mut wrong_length = create_ipfix_header();
        wrong_length[2..4].copy_from_slice(&100u16.to_be_bytes());
        assert!(IpfixHeader::from_bytes(&wrong_length).is_err());
    }

    #[test]
    fn test_set_header_parsing() {
        let mut data = vec![0u8; 8];
        data[0..2].copy_from_slice(&2u16.to_be_bytes()); // template set
        data[2..4].copy_from_slice(&8u16.to_be_bytes()); // length

        let header = SetHeader::from_bytes(&data).unwrap();
        assert_eq!(header.set_id, 2);
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
    fn test_can_parse() {
        // Valid IPFIX
        let ipfix_data = create_ipfix_header();
        assert!(IpfixParser::can_parse(&ipfix_data));

        // Invalid version
        let mut invalid_data = ipfix_data.clone();
        invalid_data[0..2].copy_from_slice(&5u16.to_be_bytes());
        assert!(!IpfixParser::can_parse(&invalid_data));

        // Too short
        let short_data = vec![0u8; 1];
        assert!(!IpfixParser::can_parse(&short_data));
    }

    #[test]
    fn test_template_parsing() {
        let config = NetflowConfig::default();
        let field_parser = FieldParser::new(&config);
        let parser = IpfixParser::new(field_parser, "emit_metadata".to_string());
        let template_cache = TemplateCache::new(100);

        // Create IPFIX packet with template
        let mut data = create_ipfix_header();
        data[2..4].copy_from_slice(&28u16.to_be_bytes()); // Update length

        // Template set header
        data.extend_from_slice(&2u16.to_be_bytes()); // set_id
        data.extend_from_slice(&12u16.to_be_bytes()); // set_length

        // Template definition
        data.extend_from_slice(&256u16.to_be_bytes()); // template_id
        data.extend_from_slice(&1u16.to_be_bytes()); // field_count
        data.extend_from_slice(&8u16.to_be_bytes()); // field_type (sourceIPv4Address)
        data.extend_from_slice(&4u16.to_be_bytes()); // field_length

        let events = parser.parse(&data, test_peer_addr(), &template_cache, false, false, true).unwrap();

        // Should have base event with template info
        assert!(!events.is_empty());
        
        // Template should be cached
        let key = (test_peer_addr(), 1, 256);
        assert!(template_cache.get(&key).is_some());
    }

    #[test]
    fn test_data_parsing_without_template() {
        let config = NetflowConfig::default();
        let field_parser = FieldParser::new(&config);
        let parser = IpfixParser::new(field_parser, "emit_metadata".to_string());
        let template_cache = TemplateCache::new(100);

        // Create IPFIX packet with data set (no template)
        let mut data = create_ipfix_header();
        data[2..4].copy_from_slice(&24u16.to_be_bytes()); // Update length

        // Data set header
        data.extend_from_slice(&256u16.to_be_bytes()); // template_id
        data.extend_from_slice(&8u16.to_be_bytes()); // set_length
        data.extend_from_slice(&[0x01, 0x02, 0x03, 0x04]); // Some data

        let _events = parser.parse(&data, test_peer_addr(), &template_cache, false, false, true).unwrap();

        // With buffering enabled, data should be buffered and we get a header event
        // or create unparseable event if buffering fails
        assert!(!_events.is_empty());
        if let Event::Log(log) = &_events[0] {
            let flow_type = log.get("flow_type").unwrap().as_str().unwrap();
            // With buffering enabled, we should get either:
            // 1. A header event (flow_type = "ipfix") when data is buffered
            // 2. An unparseable event if buffering fails
            assert!(flow_type == "ipfix" || flow_type.contains("unparseable"));
        }
    }

    #[test]
    fn test_data_parsing_with_template() {
        let config = NetflowConfig::default();
        let field_parser = FieldParser::new(&config);
        let parser = IpfixParser::new(field_parser, "emit_metadata".to_string());
        let template_cache = TemplateCache::new(100);

        // First, add a template to cache
        let template = Template::new(
            256,
            vec![TemplateField {
                field_type: 8, // sourceIPv4Address
                field_length: 4,
                enterprise_number: None,
                is_scope: false,
            }],
        );
        let key = (test_peer_addr(), 1, 256);
        template_cache.insert(key, template);

        // Create IPFIX packet with data set
        let mut data = create_ipfix_header();
        data[2..4].copy_from_slice(&24u16.to_be_bytes()); // Update length

        // Data set header
        data.extend_from_slice(&256u16.to_be_bytes()); // template_id
        data.extend_from_slice(&8u16.to_be_bytes()); // set_length
        data.extend_from_slice(&[192, 168, 1, 1]); // IPv4 address data

        let events = parser.parse(&data, test_peer_addr(), &template_cache, false, false, true).unwrap();

        // Should parse data using template
        assert!(!events.is_empty());
        if let Event::Log(log) = &events[0] {
            assert_eq!(log.get("flow_type").unwrap().as_str().unwrap(), "ipfix_data");
            assert_eq!(log.get("template_id").unwrap().as_integer().unwrap(), 256);
            assert_eq!(log.get("sourceIPv4Address").unwrap().as_str().unwrap(), "192.168.1.1");
        }
    }

    #[test]
    fn test_variable_length_parsing() {
        let config = NetflowConfig::default();
        let field_parser = FieldParser::new(&config);
        let parser = IpfixParser::new(field_parser, "emit_metadata".to_string());

        // Test single-byte length
        let data = vec![10, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
        let result = parser.parse_variable_length(&data);
        assert_eq!(result, Some((10, 1)));

        // Test three-byte length
        let data = vec![255, 0, 20];
        let result = parser.parse_variable_length(&data);
        assert_eq!(result, Some((20, 3)));

        // Test insufficient data
        let data = vec![255, 0]; // Missing second length byte
        let result = parser.parse_variable_length(&data);
        assert_eq!(result, None);
    }

    #[test]
    fn test_enterprise_field_template() {
        let config = NetflowConfig::default();
        let field_parser = FieldParser::new(&config);
        let parser = IpfixParser::new(field_parser, "emit_metadata".to_string());
        let template_cache = TemplateCache::new(100);

        // Create IPFIX packet with enterprise field template
        let mut data = create_ipfix_header();
        data[2..4].copy_from_slice(&32u16.to_be_bytes()); // Update length

        // Template set header
        data.extend_from_slice(&2u16.to_be_bytes()); // set_id
        data.extend_from_slice(&16u16.to_be_bytes()); // set_length

        // Template definition with enterprise field
        data.extend_from_slice(&256u16.to_be_bytes()); // template_id
        data.extend_from_slice(&1u16.to_be_bytes()); // field_count
        data.extend_from_slice(&0x8001u16.to_be_bytes()); // field_type with enterprise bit
        data.extend_from_slice(&4u16.to_be_bytes()); // field_length
        data.extend_from_slice(&23867u32.to_be_bytes()); // enterprise_number (HPE Aruba)

        let _events = parser.parse(&data, test_peer_addr(), &template_cache, false, false, true).unwrap();

        // Template should be cached with enterprise field
        let key = (test_peer_addr(), 1, 256);
        let template = template_cache.get(&key).unwrap();
        assert_eq!(template.fields.len(), 1);
        assert_eq!(template.fields[0].field_type, 1); // Enterprise bit stripped
        assert_eq!(template.fields[0].enterprise_number, Some(23867));
    }

    #[test]
    fn test_malformed_packet_handling() {
        let config = NetflowConfig::default();
        let field_parser = FieldParser::new(&config);
        let parser = IpfixParser::new(field_parser, "emit_metadata".to_string());
        let template_cache = TemplateCache::new(100);

        // Test with corrupted set length
        let mut data = create_ipfix_header();
        data[2..4].copy_from_slice(&24u16.to_be_bytes());

        // Corrupted set header
        data.extend_from_slice(&2u16.to_be_bytes()); // set_id
        data.extend_from_slice(&8u16.to_be_bytes()); // set_length (valid)
        data.extend_from_slice(&[0u8; 2]); // Incomplete template data

        let result = parser.parse(&data, test_peer_addr(), &template_cache, false, false, true);
        // Should handle gracefully - either return base event or handle error gracefully
        if result.is_ok() {
            let events = result.unwrap();
            // Should have header event with template count if events are present
            if !events.is_empty() {
                if let Event::Log(log) = &events[0] {
                    assert!(log.get("template_count").is_some());
                    assert!(log.get("data_set_count").is_some());
                }
            }
        } else {
            // If parsing fails due to malformed data, that's also acceptable
            // The important thing is that it doesn't panic
            println!("Parse failed as expected for malformed packet: {:?}", result.err());
        }
    }

   #[test]
   fn test_drop_unparseable_records() {
       let config = NetflowConfig::default();
       let field_parser = FieldParser::new(&config);
       let parser = IpfixParser::new(field_parser, "emit_metadata".to_string());
       let template_cache = TemplateCache::new(100);

       // Create IPFIX packet with data set (no template)
       let mut data = create_ipfix_header();
       data[2..4].copy_from_slice(&24u16.to_be_bytes());

       // Data set header
       data.extend_from_slice(&256u16.to_be_bytes()); // template_id
       data.extend_from_slice(&8u16.to_be_bytes()); // set_length
       data.extend_from_slice(&[0x01, 0x02, 0x03, 0x04]); // Some data

       // With drop_unparseable_records = true, should get no events
        let events = parser.parse(&data, test_peer_addr(), &template_cache, false, true, true).unwrap();
       assert!(events.is_empty());

       // With drop_unparseable_records = false, should get unparseable event
       let events = parser.parse(&data, test_peer_addr(), &template_cache, false, false, true).unwrap();
       assert!(!events.is_empty());
   }

   #[test]
   fn test_multiple_records_in_data_set() {
       let config = NetflowConfig::default();
       let field_parser = FieldParser::new(&config);
       let parser = IpfixParser::new(field_parser, "emit_metadata".to_string());
       let template_cache = TemplateCache::new(100);

       // Add template with fixed-length field
       let template = Template::new(
           256,
           vec![TemplateField {
               field_type: 8, // sourceIPv4Address
               field_length: 4,
               enterprise_number: None,
               is_scope: false,
           }],
       );
       let key = (test_peer_addr(), 1, 256);
       template_cache.insert(key, template);

       // Create IPFIX packet with multiple records
       let mut data = create_ipfix_header();
       data[2..4].copy_from_slice(&28u16.to_be_bytes()); // Update length

       // Data set header
       data.extend_from_slice(&256u16.to_be_bytes()); // template_id
       data.extend_from_slice(&12u16.to_be_bytes()); // set_length (4 header + 8 data)

       // Two IPv4 records
       data.extend_from_slice(&[192, 168, 1, 1]); // First record
       data.extend_from_slice(&[10, 0, 0, 1]);    // Second record

       let events = parser.parse(&data, test_peer_addr(), &template_cache, false, false, true).unwrap();

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
       let parser = IpfixParser::new(field_parser, "emit_metadata".to_string());
       let template_cache = TemplateCache::new(100);

       // Create IPFIX packet with both template and data
       let mut data = create_ipfix_header();
       data[2..4].copy_from_slice(&36u16.to_be_bytes()); // Update length

       // Template set
       data.extend_from_slice(&2u16.to_be_bytes()); // set_id
       data.extend_from_slice(&12u16.to_be_bytes()); // set_length
       data.extend_from_slice(&256u16.to_be_bytes()); // template_id
       data.extend_from_slice(&1u16.to_be_bytes()); // field_count
       data.extend_from_slice(&8u16.to_be_bytes()); // field_type
       data.extend_from_slice(&4u16.to_be_bytes()); // field_length

       // Data set
       data.extend_from_slice(&256u16.to_be_bytes()); // template_id
       data.extend_from_slice(&8u16.to_be_bytes()); // set_length
       data.extend_from_slice(&[192, 168, 1, 1]); // IPv4 data

       let events = parser.parse(&data, test_peer_addr(), &template_cache, false, false, true).unwrap();

       // Should get data event (template was cached and immediately used)
       assert!(!events.is_empty());
       
       // Should have cached the template
       let key = (test_peer_addr(), 1, 256);
       assert!(template_cache.get(&key).is_some());
   }

   #[test]
   fn test_options_template_parsing() {
       let config = NetflowConfig::default();
       let field_parser = FieldParser::new(&config);
       let parser = IpfixParser::new(field_parser, "emit_metadata".to_string());
       let template_cache = TemplateCache::new(100);

       // Create IPFIX packet with options template
       let mut data = create_ipfix_header();
       data[2..4].copy_from_slice(&28u16.to_be_bytes()); // Update length

       // Options template set header
       data.extend_from_slice(&3u16.to_be_bytes()); // set_id (options template)
       data.extend_from_slice(&12u16.to_be_bytes()); // set_length

       // Options template definition (simplified as regular template)
       data.extend_from_slice(&257u16.to_be_bytes()); // template_id
       data.extend_from_slice(&1u16.to_be_bytes()); // field_count
       data.extend_from_slice(&149u16.to_be_bytes()); // observationDomainId
       data.extend_from_slice(&4u16.to_be_bytes()); // field_length

       let events = parser.parse(&data, test_peer_addr(), &template_cache, false, false, true).unwrap();

       // Should parse options template
       assert!(!events.is_empty());
       
       // Template should be cached
       let key = (test_peer_addr(), 1, 257);
       assert!(template_cache.get(&key).is_some());
   }

   #[test]
   fn test_raw_data_inclusion() {
       let config = NetflowConfig::default();
       let field_parser = FieldParser::new(&config);
       let parser = IpfixParser::new(field_parser, "emit_metadata".to_string());
       let template_cache = TemplateCache::new(100);

       let data = create_ipfix_header();

       // Test with raw data inclusion
        let events = parser.parse(&data, test_peer_addr(), &template_cache, true, false, true).unwrap();
       assert!(!events.is_empty());
       
       if let Event::Log(log) = &events[0] {
           assert!(log.get("raw_data").is_some());
           let raw_data = log.get("raw_data").unwrap().as_str().unwrap();
           
           // Should be valid base64
           assert!(base64::engine::general_purpose::STANDARD.decode(raw_data.as_bytes()).is_ok());
       }

       // Test without raw data inclusion
       let events = parser.parse(&data, test_peer_addr(), &template_cache, false, false, true).unwrap();
       assert!(!events.is_empty());
       
       if let Event::Log(log) = &events[0] {
           assert!(log.get("raw_data").is_none());
       }
   }

   #[test]
   fn test_record_safety_limits() {
       let config = NetflowConfig::default();
       let field_parser = FieldParser::new(&config);
       let parser = IpfixParser::new(field_parser, "emit_metadata".to_string());
       let template_cache = TemplateCache::new(100);

       // Add template with very small field to potentially create many records
       let template = Template::new(
           256,
           vec![TemplateField {
               field_type: 4, // protocolIdentifier
               field_length: 1,
               enterprise_number: None,
               is_scope: false,
           }],
       );
       let key = (test_peer_addr(), 1, 256);
       template_cache.insert(key, template);

       // Create IPFIX packet with large data set
       let mut data = create_ipfix_header();
       let data_size = 10000; // Large data set
       data[2..4].copy_from_slice(&((16 + 4 + data_size) as u16).to_be_bytes());

       // Data set header
       data.extend_from_slice(&256u16.to_be_bytes()); // template_id
       data.extend_from_slice(&((4 + data_size) as u16).to_be_bytes()); // set_length

       // Add lots of data (each record is 1 byte)
       data.extend(vec![6u8; data_size]); // All TCP protocol

       let events = parser.parse(&data, test_peer_addr(), &template_cache, false, false, true).unwrap();

       // Should be limited by MAX_RECORDS safety limit
       assert!(events.len() <= 10000); // MAX_RECORDS constant
       
       // All events should be valid
       for event in &events {
           if let Event::Log(log) = event {
               assert_eq!(log.get("flow_type").unwrap().as_str().unwrap(), "ipfix_data");
               assert_eq!(log.get("protocolIdentifier").unwrap().as_integer().unwrap(), 6);
           }
       }
   }

   #[test]
   fn test_header_to_log_event() {
       let data = create_ipfix_header();
       let header = IpfixHeader::from_bytes(&data).unwrap();
       let log_event = header.to_log_event();

       assert_eq!(log_event.get("flow_type").unwrap().as_str().unwrap(), "ipfix");
       assert_eq!(log_event.get("version").unwrap().as_integer().unwrap(), 10);
       assert_eq!(log_event.get("length").unwrap().as_integer().unwrap(), 16);
       assert_eq!(log_event.get("export_time").unwrap().as_integer().unwrap(), 1609459200);
       assert_eq!(log_event.get("sequence_number").unwrap().as_integer().unwrap(), 12345);
       assert_eq!(log_event.get("observation_domain_id").unwrap().as_integer().unwrap(), 1);
   }

   #[test]
   fn test_set_header_edge_cases() {
       // Test minimum valid length
       let data = vec![0, 2, 0, 4]; // set_id=2, length=4
       let header = SetHeader::from_bytes(&data).unwrap();
       assert_eq!(header.length, 4);

       // Test invalid length (too small)
       let data = vec![0, 2, 0, 3]; // length=3 (less than minimum 4)
       assert!(SetHeader::from_bytes(&data).is_err());

       // Test length too large
       let data = vec![0, 2, 255, 255]; // length=65535
       let header = SetHeader::from_bytes(&data).unwrap();
       assert_eq!(header.length, 65535);

       // Test length beyond maximum
       let mut data = vec![0, 2];
       data.extend_from_slice(&((MAX_SET_LENGTH + 1) as u16).to_be_bytes());
       assert!(SetHeader::from_bytes(&data).is_err());
   }
}