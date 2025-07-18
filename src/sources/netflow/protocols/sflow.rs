//! sFlow protocol parser.
//!
//! sFlow (Sampled Flow) is a packet sampling technology for monitoring
//! network traffic. Unlike NetFlow which exports flow records, sFlow
//! exports raw packet samples and interface counters at regular intervals.

use crate::sources::netflow::events::*;
use std::net::SocketAddr;
use vector_lib::event::{Event, LogEvent};


/// sFlow protocol constants
const SFLOW_VERSION: u32 = 5;
const SFLOW_HEADER_SIZE: usize = 28;
const MAX_SAMPLES: u32 = 1000; // Sanity check for sample count

/// sFlow datagram header structure
#[derive(Debug, Clone)]
pub struct SflowHeader {
    pub version: u32,
    pub agent_address_type: u32,
    pub agent_address: [u8; 16], // Support both IPv4 and IPv6
    pub sub_agent_id: u32,
    pub sequence_number: u32,
    pub sys_uptime: u32,
    pub num_samples: u32,
}

impl SflowHeader {
    /// Parse sFlow header from packet data
    pub fn from_bytes(data: &[u8]) -> Result<Self, String> {
        if data.len() < SFLOW_HEADER_SIZE {
            return Err(format!(
                "Packet too short for sFlow header: {} bytes, need at least {}",
                data.len(),
                SFLOW_HEADER_SIZE
            ));
        }

        let version = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);
        if version != SFLOW_VERSION {
            return Err(format!(
                "Unsupported sFlow version: {}, expected {}",
                version, SFLOW_VERSION
            ));
        }

        let agent_address_type = u32::from_be_bytes([data[4], data[5], data[6], data[7]]);
        
        // Parse agent address based on type
        let mut agent_address = [0u8; 16];
        let address_size = match agent_address_type {
            1 => { // IPv4
                if data.len() < 12 + 4 {
                    return Err("Insufficient data for IPv4 agent address".to_string());
                }
                agent_address[12..16].copy_from_slice(&data[8..12]);
                4
            }
            2 => { // IPv6
                if data.len() < 8 + 16 {
                    return Err("Insufficient data for IPv6 agent address".to_string());
                }
                agent_address.copy_from_slice(&data[8..24]);
                16
            }
            _ => {
                return Err(format!("Unsupported agent address type: {}", agent_address_type));
            }
        };

        let base_offset = 8 + address_size;
        if data.len() < base_offset + 16 {
            return Err("Insufficient data for sFlow header".to_string());
        }

        let sub_agent_id = u32::from_be_bytes([
            data[base_offset], data[base_offset + 1], 
            data[base_offset + 2], data[base_offset + 3]
        ]);
        let sequence_number = u32::from_be_bytes([
            data[base_offset + 4], data[base_offset + 5], 
            data[base_offset + 6], data[base_offset + 7]
        ]);
        let sys_uptime = u32::from_be_bytes([
            data[base_offset + 8], data[base_offset + 9], 
            data[base_offset + 10], data[base_offset + 11]
        ]);
        let num_samples = u32::from_be_bytes([
            data[base_offset + 12], data[base_offset + 13], 
            data[base_offset + 14], data[base_offset + 15]
        ]);

        if num_samples > MAX_SAMPLES {
            return Err(format!(
                "Unreasonable sample count: {}, maximum expected {}",
                num_samples, MAX_SAMPLES
            ));
        }

        Ok(Self {
            version,
            agent_address_type,
            agent_address,
            sub_agent_id,
            sequence_number,
            sys_uptime,
            num_samples,
        })
    }

    /// Get agent address as string
    pub fn agent_address_string(&self) -> String {
        match self.agent_address_type {
            1 => { // IPv4
                format!(
                    "{}.{}.{}.{}",
                    self.agent_address[12], self.agent_address[13],
                    self.agent_address[14], self.agent_address[15]
                )
            }
            2 => { // IPv6
                format!(
                    "{:02x}{:02x}:{:02x}{:02x}:{:02x}{:02x}:{:02x}{:02x}:{:02x}{:02x}:{:02x}{:02x}:{:02x}{:02x}:{:02x}{:02x}",
                    self.agent_address[0], self.agent_address[1], self.agent_address[2], self.agent_address[3],
                    self.agent_address[4], self.agent_address[5], self.agent_address[6], self.agent_address[7],
                    self.agent_address[8], self.agent_address[9], self.agent_address[10], self.agent_address[11],
                    self.agent_address[12], self.agent_address[13], self.agent_address[14], self.agent_address[15]
                )
            }
            _ => "unknown".to_string(),
        }
    }

    /// Get header size in bytes
    pub fn header_size(&self) -> usize {
        match self.agent_address_type {
            1 => 28, // IPv4: 8 + 4 + 16 = 28
            2 => 40, // IPv6: 8 + 16 + 16 = 40
            _ => 28,
        }
    }

    /// Create base log event with header information
    pub fn to_log_event(&self) -> LogEvent {
        let mut log_event = LogEvent::default();
        log_event.insert("flow_type", "sflow");
        log_event.insert("version", self.version);
        log_event.insert("agent_address_type", self.agent_address_type);
        log_event.insert("agent_address", self.agent_address_string());
        log_event.insert("sub_agent_id", self.sub_agent_id);
        log_event.insert("sequence_number", self.sequence_number);
        log_event.insert("sys_uptime", self.sys_uptime);
        log_event.insert("num_samples", self.num_samples);
        log_event
    }
}

/// sFlow sample types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SampleType {
    FlowSample = 1,
    CounterSample = 2,
    ExpandedFlowSample = 3,
    ExpandedCounterSample = 4,
}

impl SampleType {
    pub fn from_u32(value: u32) -> Option<Self> {
        match value {
            1 => Some(SampleType::FlowSample),
            2 => Some(SampleType::CounterSample),
            3 => Some(SampleType::ExpandedFlowSample),
            4 => Some(SampleType::ExpandedCounterSample),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            SampleType::FlowSample => "flow_sample",
            SampleType::CounterSample => "counter_sample",
            SampleType::ExpandedFlowSample => "expanded_flow_sample",
            SampleType::ExpandedCounterSample => "expanded_counter_sample",
        }
    }
}

/// sFlow sample header
#[derive(Debug, Clone)]
pub struct SampleHeader {
    pub sample_type: SampleType,
    pub sample_length: u32,
}

impl SampleHeader {
    pub fn from_bytes(data: &[u8]) -> Result<Self, String> {
        if data.len() < 8 {
            return Err("Insufficient data for sample header".to_string());
        }

        let sample_type_raw = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);
        let sample_type = SampleType::from_u32(sample_type_raw)
            .ok_or_else(|| format!("Unknown sample type: {}", sample_type_raw))?;

        let sample_length = u32::from_be_bytes([data[4], data[5], data[6], data[7]]);

        if sample_length < 8 {
            return Err(format!("Invalid sample length: {}", sample_length));
        }

        Ok(Self {
            sample_type,
            sample_length,
        })
    }
}

/// sFlow flow sample structure
#[derive(Debug, Clone)]
pub struct FlowSample {
    pub sequence_number: u32,
    pub source_id_type: u8,
    pub source_id_index: u32,
    pub sampling_rate: u32,
    pub sample_pool: u32,
    pub drops: u32,
    pub input_interface: u32,
    pub output_interface: u32,
    pub flow_records: Vec<FlowRecord>,
}

/// sFlow counter sample structure
#[derive(Debug, Clone)]
pub struct CounterSample {
    pub sequence_number: u32,
    pub source_id_type: u8,
    pub source_id_index: u32,
    pub counter_records: Vec<CounterRecord>,
}

/// sFlow flow record
#[derive(Debug, Clone)]
pub struct FlowRecord {
    pub record_type: u32,
    pub record_length: u32,
    pub data: Vec<u8>,
}

/// sFlow counter record
#[derive(Debug, Clone)]
pub struct CounterRecord {
    pub record_type: u32,
    pub record_length: u32,
    pub data: Vec<u8>,
}

/// sFlow protocol parser
pub struct SflowParser;

impl SflowParser {
    /// Create a new sFlow parser
    pub fn new() -> Self {
        Self
    }

    /// Check if packet data looks like sFlow
    pub fn can_parse(data: &[u8]) -> bool {
        if data.len() < 4 {
            return false;
        }

        let version = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);
        if version != SFLOW_VERSION {
            return false;
        }

        // Additional validation
        if data.len() < 8 {
            return false;
        }

        let agent_address_type = u32::from_be_bytes([data[4], data[5], data[6], data[7]]);
        if agent_address_type != 1 && agent_address_type != 2 {
            return false;
        }

        true
    }

    /// Parse sFlow packet and return events
    pub fn parse(
        &self,
        data: &[u8],
        peer_addr: SocketAddr,
        include_raw_data: bool,
    ) -> Result<Vec<Event>, String> {
        let mut events = Vec::new();

        // Parse header
        let header = SflowHeader::from_bytes(data)?;
        
        debug!(
            "Parsing sFlow packet: version={}, samples={}, agent={}",
            header.version, header.num_samples, header.agent_address_string()
        );

        // Create base event with header info
        let mut base_event = header.to_log_event();
        if include_raw_data {
            let encoded = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, data);
            base_event.insert("raw_data", encoded);
        }

        // Parse samples
        let mut offset = header.header_size();
        let mut flow_samples = 0;
        let mut counter_samples = 0;
        let mut unknown_samples = 0;

        for sample_index in 0..header.num_samples {
            if offset + 8 > data.len() {
                warn!("Insufficient data for sample {} header", sample_index);
                break;
            }

            let sample_header = match SampleHeader::from_bytes(&data[offset..]) {
                Ok(header) => header,
                Err(e) => {
                    warn!("Invalid sample header at index {}: {}", sample_index, e);
                    unknown_samples += 1;
                    break;
                }
            };

            let sample_end = offset + sample_header.sample_length as usize;
            if sample_end > data.len() {
                warn!(
                    "Sample {} extends beyond packet boundary: offset={}, length={}, packet_size={}",
                    sample_index, offset, sample_header.sample_length, data.len()
                );
                break;
            }

            let sample_data = &data[offset..sample_end];

            match sample_header.sample_type {
                SampleType::FlowSample | SampleType::ExpandedFlowSample => {
                    match self.parse_flow_sample(sample_data, sample_index, &header, peer_addr) {
                        Ok(mut sample_events) => {
                            flow_samples += 1;
                            events.append(&mut sample_events);
                        }
                        Err(e) => {
                            warn!("Failed to parse flow sample {}: {}", sample_index, e);
                            unknown_samples += 1;
                        }
                    }
                }
                SampleType::CounterSample | SampleType::ExpandedCounterSample => {
                    match self.parse_counter_sample(sample_data, sample_index, &header, peer_addr) {
                        Ok(mut sample_events) => {
                            counter_samples += 1;
                            events.append(&mut sample_events);
                        }
                        Err(e) => {
                            warn!("Failed to parse counter sample {}: {}", sample_index, e);
                            unknown_samples += 1;
                        }
                    }
                }
            }

            offset = sample_end;
        }

        // If no sample events were generated, include the header event
        if events.is_empty() {
            base_event.insert("flow_samples", flow_samples);
            base_event.insert("counter_samples", counter_samples);
            base_event.insert("unknown_samples", unknown_samples);
            events.push(Event::Log(base_event));
        }

        emit!(SflowPacketProcessed {
            peer_addr,
            flow_samples,
            counter_samples,
            unknown_samples,
            event_count: events.len(),
        });

        Ok(events)
    }

    /// Parse flow sample
    fn parse_flow_sample(
        &self,
        data: &[u8],
        sample_index: u32,
        header: &SflowHeader,
        peer_addr: SocketAddr,
    ) -> Result<Vec<Event>, String> {
        if data.len() < 32 {
            return Err("Insufficient data for flow sample".to_string());
        }

        let sequence_number = u32::from_be_bytes([data[8], data[9], data[10], data[11]]);
        let source_id = u32::from_be_bytes([data[12], data[13], data[14], data[15]]);
        let source_id_type = ((source_id >> 24) & 0xFF) as u8;
        let source_id_index = source_id & 0x00FFFFFF;
        let sampling_rate = u32::from_be_bytes([data[16], data[17], data[18], data[19]]);
        let sample_pool = u32::from_be_bytes([data[20], data[21], data[22], data[23]]);
        let drops = u32::from_be_bytes([data[24], data[25], data[26], data[27]]);
        let input_interface = u32::from_be_bytes([data[28], data[29], data[30], data[31]]);

        let mut log_event = LogEvent::default();
        log_event.insert("flow_type", "sflow_flow_sample");
        log_event.insert("sample_index", sample_index);
        log_event.insert("agent_address", header.agent_address_string());
        log_event.insert("sequence_number", sequence_number);
        log_event.insert("source_id_type", source_id_type);
        log_event.insert("source_id_index", source_id_index);
        log_event.insert("sampling_rate", sampling_rate);
        log_event.insert("sample_pool", sample_pool);
        log_event.insert("drops", drops);
        log_event.insert("input_interface", input_interface);

        // Parse output interface (if present)
        if data.len() >= 36 {
            let output_interface = u32::from_be_bytes([data[32], data[33], data[34], data[35]]);
            log_event.insert("output_interface", output_interface);
        }

        // Parse flow records (simplified for now)
        if data.len() >= 40 {
            let num_flow_records = u32::from_be_bytes([data[36], data[37], data[38], data[39]]);
            log_event.insert("num_flow_records", num_flow_records);

            // Parse individual flow records
            let mut record_offset = 40;
            let mut records_parsed = 0;

            for _record_index in 0..num_flow_records.min(100) { // Limit to prevent DoS
                if record_offset + 8 > data.len() {
                    break;
                }

                let record_type = u32::from_be_bytes([
                    data[record_offset], data[record_offset + 1], 
                    data[record_offset + 2], data[record_offset + 3]
                ]);
                let record_length = u32::from_be_bytes([
                    data[record_offset + 4], data[record_offset + 5], 
                    data[record_offset + 6], data[record_offset + 7]
                ]);

                let record_end = record_offset + 8 + record_length as usize;
                if record_end > data.len() {
                    break;
                }

                // Parse specific record types
                match record_type {
                    1 => { // Raw packet header
                        self.parse_raw_packet_header(
                            &data[record_offset + 8..record_end],
                            &mut log_event,
                        );
                    }
                    2 => { // Ethernet frame data
                        self.parse_ethernet_frame_data(
                            &data[record_offset + 8..record_end],
                            &mut log_event,
                        );
                    }
                    1001 => { // Extended switch data
                        self.parse_extended_switch_data(
                            &data[record_offset + 8..record_end],
                            &mut log_event,
                        );
                    }
                    1002 => { // Extended router data
                        self.parse_extended_router_data(
                            &data[record_offset + 8..record_end],
                            &mut log_event,
                        );
                    }
                    _ => {
                        // Unknown record type
                        debug!("Unknown flow record type: {}", record_type);
                    }
                }

                record_offset = record_end;
                records_parsed += 1;
            }

            log_event.insert("records_parsed", records_parsed);
        }

        emit!(DataRecordParsed {
            template_id: 0, // sFlow doesn't use templates
            fields_parsed: log_event.as_map().unwrap().len(),
            record_size: data.len(),
            peer_addr,
            protocol: "sflow",
        });

        Ok(vec![Event::Log(log_event)])
    }

    /// Parse counter sample
    fn parse_counter_sample(
        &self,
        data: &[u8],
        sample_index: u32,
        header: &SflowHeader,
        _peer_addr: SocketAddr,
    ) -> Result<Vec<Event>, String> {
        if data.len() < 20 {
            return Err("Insufficient data for counter sample".to_string());
        }

        let sequence_number = u32::from_be_bytes([data[8], data[9], data[10], data[11]]);
        let source_id = u32::from_be_bytes([data[12], data[13], data[14], data[15]]);
        let source_id_type = ((source_id >> 24) & 0xFF) as u8;
        let source_id_index = source_id & 0x00FFFFFF;
        let num_counter_records = u32::from_be_bytes([data[16], data[17], data[18], data[19]]);

        let mut log_event = LogEvent::default();
        log_event.insert("flow_type", "sflow_counter_sample");
        log_event.insert("sample_index", sample_index);
        log_event.insert("agent_address", header.agent_address_string());
        log_event.insert("sequence_number", sequence_number);
        log_event.insert("source_id_type", source_id_type);
        log_event.insert("source_id_index", source_id_index);
        log_event.insert("num_counter_records", num_counter_records);

        // Parse counter records
        let mut record_offset = 20;
        let mut records_parsed = 0;

        for _record_index in 0..num_counter_records.min(50) { // Limit to prevent DoS
            if record_offset + 8 > data.len() {
                break;
            }

            let record_type = u32::from_be_bytes([
                data[record_offset], data[record_offset + 1], 
                data[record_offset + 2], data[record_offset + 3]
            ]);
            let record_length = u32::from_be_bytes([
                data[record_offset + 4], data[record_offset + 5], 
                data[record_offset + 6], data[record_offset + 7]
            ]);

            let record_end = record_offset + 8 + record_length as usize;
            if record_end > data.len() {
                break;
            }

            // Parse specific counter record types
            match record_type {
                1 => { // Generic interface counters
                    self.parse_interface_counters(
                        &data[record_offset + 8..record_end],
                        &mut log_event,
                    );
                }
                2 => { // Ethernet interface counters
                    self.parse_ethernet_counters(
                        &data[record_offset + 8..record_end],
                        &mut log_event,
                    );
                }
                _ => {
                    debug!("Unknown counter record type: {}", record_type);
                }
            }

            record_offset = record_end;
            records_parsed += 1;
        }

        log_event.insert("records_parsed", records_parsed);

        Ok(vec![Event::Log(log_event)])
    }

    /// Parse raw packet header record
    fn parse_raw_packet_header(&self, data: &[u8], log_event: &mut LogEvent) {
        if data.len() < 16 {
            return;
        }

        let header_protocol = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);
        let frame_length = u32::from_be_bytes([data[4], data[5], data[6], data[7]]);
        let payload_removed = u32::from_be_bytes([data[8], data[9], data[10], data[11]]);
        let header_length = u32::from_be_bytes([data[12], data[13], data[14], data[15]]);

        log_event.insert("header_protocol", header_protocol);
        log_event.insert("frame_length", frame_length);
        log_event.insert("payload_removed", payload_removed);
        log_event.insert("header_length", header_length);

        // Parse packet header if present
        if data.len() > 16 && header_length > 0 {
            let header_end = 16 + header_length as usize;
            if header_end <= data.len() {
                let packet_header = &data[16..header_end];
                self.parse_packet_header(packet_header, log_event, header_protocol);
            }
        }
    }

    /// Parse Ethernet frame data record
    fn parse_ethernet_frame_data(&self, data: &[u8], log_event: &mut LogEvent) {
        if data.len() < 4 {
            return;
        }

        let ethernet_length = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);
        log_event.insert("ethernet_length", ethernet_length);

        // Parse Ethernet header if present
        if data.len() >= 18 && ethernet_length >= 14 {
            let eth_header = &data[4..18]; // 14 bytes Ethernet header
            
            // Destination MAC
            let dst_mac = format!(
                "{:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
                eth_header[0], eth_header[1], eth_header[2],
                eth_header[3], eth_header[4], eth_header[5]
            );
            log_event.insert("dst_mac", dst_mac);

            // Source MAC
            let src_mac = format!(
                "{:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
                eth_header[6], eth_header[7], eth_header[8],
                eth_header[9], eth_header[10], eth_header[11]
            );
            log_event.insert("src_mac", src_mac);

            // EtherType
            let ethertype = u16::from_be_bytes([eth_header[12], eth_header[13]]);
            log_event.insert("ethertype", ethertype);
            
            // Common EtherType names
            let ethertype_name = match ethertype {
                0x0800 => "IPv4",
                0x0806 => "ARP",
                0x86DD => "IPv6",
                0x8100 => "VLAN",
                0x8847 => "MPLS",
                _ => "Other",
            };
            log_event.insert("ethertype_name", ethertype_name);
        }
    }

    /// Parse extended switch data record
    fn parse_extended_switch_data(&self, data: &[u8], log_event: &mut LogEvent) {
        if data.len() < 16 {
            return;
        }

        let src_vlan = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);
        let src_priority = u32::from_be_bytes([data[4], data[5], data[6], data[7]]);
        let dst_vlan = u32::from_be_bytes([data[8], data[9], data[10], data[11]]);
        let dst_priority = u32::from_be_bytes([data[12], data[13], data[14], data[15]]);

        log_event.insert("src_vlan", src_vlan);
        log_event.insert("src_priority", src_priority);
        log_event.insert("dst_vlan", dst_vlan);
        log_event.insert("dst_priority", dst_priority);
    }

    /// Parse extended router data record
    fn parse_extended_router_data(&self, data: &[u8], log_event: &mut LogEvent) {
        if data.len() < 20 {
            return;
        }

        let next_hop_type = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);
        log_event.insert("next_hop_type", next_hop_type);

        match next_hop_type {
            1 => { // IPv4
                if data.len() >= 8 {
                    let next_hop = u32::from_be_bytes([data[4], data[5], data[6], data[7]]);
                    let next_hop_str = format!(
                        "{}.{}.{}.{}",
                        (next_hop >> 24) & 0xFF,
                        (next_hop >> 16) & 0xFF,
                        (next_hop >> 8) & 0xFF,
                        next_hop & 0xFF
                    );
                    log_event.insert("next_hop", next_hop_str);
                }
            }
            2 => { // IPv6
                if data.len() >= 20 {
                    let next_hop = &data[4..20];
                    let next_hop_str = format!(
                        "{:02x}{:02x}:{:02x}{:02x}:{:02x}{:02x}:{:02x}{:02x}:{:02x}{:02x}:{:02x}{:02x}:{:02x}{:02x}:{:02x}{:02x}",
                        next_hop[0], next_hop[1], next_hop[2], next_hop[3],
                        next_hop[4], next_hop[5], next_hop[6], next_hop[7],
                        next_hop[8], next_hop[9], next_hop[10], next_hop[11],
                        next_hop[12], next_hop[13], next_hop[14], next_hop[15]
                    );
                    log_event.insert("next_hop", next_hop_str);
                }
            }
            _ => {}
        }

        // Parse additional router data if present
        let base_offset = match next_hop_type {
            1 => 8,  // IPv4
            2 => 20, // IPv6
            _ => 4,
        };

        if data.len() >= base_offset + 16 {
            let src_mask = u32::from_be_bytes([
                data[base_offset], data[base_offset + 1], 
                data[base_offset + 2], data[base_offset + 3]
            ]);
            let dst_mask = u32::from_be_bytes([
                data[base_offset + 4], data[base_offset + 5], 
                data[base_offset + 6], data[base_offset + 7]
            ]);

            log_event.insert("src_mask", src_mask);
            log_event.insert("dst_mask", dst_mask);
        }
    }

    /// Parse interface counters record
    fn parse_interface_counters(&self, data: &[u8], log_event: &mut LogEvent) {
        if data.len() < 88 {
            return;
        }

        let if_index = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);
        let if_type = u32::from_be_bytes([data[4], data[5], data[6], data[7]]);
       let if_speed = u64::from_be_bytes([
           data[8], data[9], data[10], data[11],
           data[12], data[13], data[14], data[15]
       ]);
       let if_direction = u32::from_be_bytes([data[16], data[17], data[18], data[19]]);
       let if_status = u32::from_be_bytes([data[20], data[21], data[22], data[23]]);

       // Input counters
       let if_in_octets = u64::from_be_bytes([
           data[24], data[25], data[26], data[27],
           data[28], data[29], data[30], data[31]
       ]);
       let if_in_ucast_pkts = u32::from_be_bytes([data[32], data[33], data[34], data[35]]);
       let if_in_mcast_pkts = u32::from_be_bytes([data[36], data[37], data[38], data[39]]);
       let if_in_bcast_pkts = u32::from_be_bytes([data[40], data[41], data[42], data[43]]);
       let if_in_discards = u32::from_be_bytes([data[44], data[45], data[46], data[47]]);
       let if_in_errors = u32::from_be_bytes([data[48], data[49], data[50], data[51]]);
       let if_in_unknown_protos = u32::from_be_bytes([data[52], data[53], data[54], data[55]]);

       // Output counters
       let if_out_octets = u64::from_be_bytes([
           data[56], data[57], data[58], data[59],
           data[60], data[61], data[62], data[63]
       ]);
       let if_out_ucast_pkts = u32::from_be_bytes([data[64], data[65], data[66], data[67]]);
       let if_out_mcast_pkts = u32::from_be_bytes([data[68], data[69], data[70], data[71]]);
       let if_out_bcast_pkts = u32::from_be_bytes([data[72], data[73], data[74], data[75]]);
       let if_out_discards = u32::from_be_bytes([data[76], data[77], data[78], data[79]]);
       let if_out_errors = u32::from_be_bytes([data[80], data[81], data[82], data[83]]);
       let if_promiscuous_mode = u32::from_be_bytes([data[84], data[85], data[86], data[87]]);

       log_event.insert("if_index", if_index);
       log_event.insert("if_type", if_type);
       log_event.insert("if_speed", if_speed as i64);
       log_event.insert("if_direction", if_direction);
       log_event.insert("if_status", if_status);
       log_event.insert("if_in_octets", if_in_octets as i64);
       log_event.insert("if_in_ucast_pkts", if_in_ucast_pkts);
       log_event.insert("if_in_mcast_pkts", if_in_mcast_pkts);
       log_event.insert("if_in_bcast_pkts", if_in_bcast_pkts);
       log_event.insert("if_in_discards", if_in_discards);
       log_event.insert("if_in_errors", if_in_errors);
       log_event.insert("if_in_unknown_protos", if_in_unknown_protos);
       log_event.insert("if_out_octets", if_out_octets as i64);
       log_event.insert("if_out_ucast_pkts", if_out_ucast_pkts);
       log_event.insert("if_out_mcast_pkts", if_out_mcast_pkts);
       log_event.insert("if_out_bcast_pkts", if_out_bcast_pkts);
       log_event.insert("if_out_discards", if_out_discards);
       log_event.insert("if_out_errors", if_out_errors);
       log_event.insert("if_promiscuous_mode", if_promiscuous_mode);

       // Calculate utilization if speed is known
       if if_speed > 0 {
           let total_octets = if_in_octets + if_out_octets;
           let utilization = (total_octets as f64 * 8.0) / if_speed as f64 * 100.0;
           log_event.insert("if_utilization_percent", utilization);
       }
   }

   /// Parse Ethernet interface counters record
   fn parse_ethernet_counters(&self, data: &[u8], log_event: &mut LogEvent) {
       if data.len() < 52 {
           return;
       }

       let dot3_stats_alignment_errors = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);
       let dot3_stats_fcs_errors = u32::from_be_bytes([data[4], data[5], data[6], data[7]]);
       let dot3_stats_single_collision_frames = u32::from_be_bytes([data[8], data[9], data[10], data[11]]);
       let dot3_stats_multiple_collision_frames = u32::from_be_bytes([data[12], data[13], data[14], data[15]]);
       let dot3_stats_sqe_test_errors = u32::from_be_bytes([data[16], data[17], data[18], data[19]]);
       let dot3_stats_deferred_transmissions = u32::from_be_bytes([data[20], data[21], data[22], data[23]]);
       let dot3_stats_late_collisions = u32::from_be_bytes([data[24], data[25], data[26], data[27]]);
       let dot3_stats_excessive_collisions = u32::from_be_bytes([data[28], data[29], data[30], data[31]]);
       let dot3_stats_internal_mac_transmit_errors = u32::from_be_bytes([data[32], data[33], data[34], data[35]]);
       let dot3_stats_carrier_sense_errors = u32::from_be_bytes([data[36], data[37], data[38], data[39]]);
       let dot3_stats_frame_too_longs = u32::from_be_bytes([data[40], data[41], data[42], data[43]]);
       let dot3_stats_internal_mac_receive_errors = u32::from_be_bytes([data[44], data[45], data[46], data[47]]);
       let dot3_stats_symbol_errors = u32::from_be_bytes([data[48], data[49], data[50], data[51]]);

       log_event.insert("dot3_stats_alignment_errors", dot3_stats_alignment_errors);
       log_event.insert("dot3_stats_fcs_errors", dot3_stats_fcs_errors);
       log_event.insert("dot3_stats_single_collision_frames", dot3_stats_single_collision_frames);
       log_event.insert("dot3_stats_multiple_collision_frames", dot3_stats_multiple_collision_frames);
       log_event.insert("dot3_stats_sqe_test_errors", dot3_stats_sqe_test_errors);
       log_event.insert("dot3_stats_deferred_transmissions", dot3_stats_deferred_transmissions);
       log_event.insert("dot3_stats_late_collisions", dot3_stats_late_collisions);
       log_event.insert("dot3_stats_excessive_collisions", dot3_stats_excessive_collisions);
       log_event.insert("dot3_stats_internal_mac_transmit_errors", dot3_stats_internal_mac_transmit_errors);
       log_event.insert("dot3_stats_carrier_sense_errors", dot3_stats_carrier_sense_errors);
       log_event.insert("dot3_stats_frame_too_longs", dot3_stats_frame_too_longs);
       log_event.insert("dot3_stats_internal_mac_receive_errors", dot3_stats_internal_mac_receive_errors);
       log_event.insert("dot3_stats_symbol_errors", dot3_stats_symbol_errors);
   }

   /// Parse packet header based on protocol
   fn parse_packet_header(&self, header_data: &[u8], log_event: &mut LogEvent, protocol: u32) {
       match protocol {
           1 => { // Ethernet
               if header_data.len() >= 14 {
                   self.parse_ethernet_header(header_data, log_event);
                   
                   // Check for IP payload
                   if header_data.len() > 14 {
                       let ethertype = u16::from_be_bytes([header_data[12], header_data[13]]);
                       match ethertype {
                           0x0800 => { // IPv4
                               if header_data.len() >= 34 { // 14 eth + 20 ip minimum
                                   self.parse_ipv4_header(&header_data[14..], log_event);
                               }
                           }
                           0x86DD => { // IPv6
                               if header_data.len() >= 54 { // 14 eth + 40 ip minimum
                                   self.parse_ipv6_header(&header_data[14..], log_event);
                               }
                           }
                           _ => {}
                       }
                   }
               }
           }
           _ => {
               debug!("Unknown header protocol: {}", protocol);
           }
       }
   }

   /// Parse Ethernet header
   fn parse_ethernet_header(&self, data: &[u8], log_event: &mut LogEvent) {
       if data.len() < 14 {
           return;
       }

       // Already parsed in parse_ethernet_frame_data, but add here for completeness
       let dst_mac = format!(
           "{:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
           data[0], data[1], data[2], data[3], data[4], data[5]
       );
       let src_mac = format!(
           "{:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
           data[6], data[7], data[8], data[9], data[10], data[11]
       );
       let ethertype = u16::from_be_bytes([data[12], data[13]]);

       log_event.insert("eth_dst", dst_mac);
       log_event.insert("eth_src", src_mac);
       log_event.insert("eth_type", ethertype);
   }

   /// Parse IPv4 header
   fn parse_ipv4_header(&self, data: &[u8], log_event: &mut LogEvent) {
       if data.len() < 20 {
           return;
       }

       let version = (data[0] >> 4) & 0x0F;
       let ihl = data[0] & 0x0F;
       let tos = data[1];
       let total_length = u16::from_be_bytes([data[2], data[3]]);
       let identification = u16::from_be_bytes([data[4], data[5]]);
       let flags = (data[6] >> 5) & 0x07;
       let fragment_offset = u16::from_be_bytes([data[6], data[7]]) & 0x1FFF;
       let ttl = data[8];
       let protocol = data[9];
       let checksum = u16::from_be_bytes([data[10], data[11]]);
       let src_addr = u32::from_be_bytes([data[12], data[13], data[14], data[15]]);
       let dst_addr = u32::from_be_bytes([data[16], data[17], data[18], data[19]]);

       log_event.insert("ip_version", version);
       log_event.insert("ip_ihl", ihl);
       log_event.insert("ip_tos", tos);
       log_event.insert("ip_total_length", total_length);
       log_event.insert("ip_identification", identification);
       log_event.insert("ip_flags", flags);
       log_event.insert("ip_fragment_offset", fragment_offset);
       log_event.insert("ip_ttl", ttl);
       log_event.insert("ip_protocol", protocol);
       log_event.insert("ip_checksum", checksum);
       
       let src_addr_str = format!(
           "{}.{}.{}.{}",
           (src_addr >> 24) & 0xFF,
           (src_addr >> 16) & 0xFF,
           (src_addr >> 8) & 0xFF,
           src_addr & 0xFF
       );
       let dst_addr_str = format!(
           "{}.{}.{}.{}",
           (dst_addr >> 24) & 0xFF,
           (dst_addr >> 16) & 0xFF,
           (dst_addr >> 8) & 0xFF,
           dst_addr & 0xFF
       );
       
       log_event.insert("ip_src", src_addr_str);
       log_event.insert("ip_dst", dst_addr_str);

       // Parse protocol-specific headers
       let header_length = (ihl * 4) as usize;
       if data.len() > header_length {
           let payload = &data[header_length..];
           match protocol {
               6 => { // TCP
                   self.parse_tcp_header(payload, log_event);
               }
               17 => { // UDP
                   self.parse_udp_header(payload, log_event);
               }
               1 => { // ICMP
                   self.parse_icmp_header(payload, log_event);
               }
               _ => {}
           }
       }
   }

   /// Parse IPv6 header
   fn parse_ipv6_header(&self, data: &[u8], log_event: &mut LogEvent) {
       if data.len() < 40 {
           return;
       }

       let version = (data[0] >> 4) & 0x0F;
       let traffic_class = ((data[0] & 0x0F) << 4) | ((data[1] >> 4) & 0x0F);
       let flow_label = u32::from_be_bytes([0, data[1] & 0x0F, data[2], data[3]]);
       let payload_length = u16::from_be_bytes([data[4], data[5]]);
       let next_header = data[6];
       let hop_limit = data[7];

       log_event.insert("ipv6_version", version);
       log_event.insert("ipv6_traffic_class", traffic_class);
       log_event.insert("ipv6_flow_label", flow_label);
       log_event.insert("ipv6_payload_length", payload_length);
       log_event.insert("ipv6_next_header", next_header);
       log_event.insert("ipv6_hop_limit", hop_limit);

       // Format IPv6 addresses
       let src_addr = format!(
           "{:02x}{:02x}:{:02x}{:02x}:{:02x}{:02x}:{:02x}{:02x}:{:02x}{:02x}:{:02x}{:02x}:{:02x}{:02x}:{:02x}{:02x}",
           data[8], data[9], data[10], data[11], data[12], data[13], data[14], data[15],
           data[16], data[17], data[18], data[19], data[20], data[21], data[22], data[23]
       );
       let dst_addr = format!(
           "{:02x}{:02x}:{:02x}{:02x}:{:02x}{:02x}:{:02x}{:02x}:{:02x}{:02x}:{:02x}{:02x}:{:02x}{:02x}:{:02x}{:02x}",
           data[24], data[25], data[26], data[27], data[28], data[29], data[30], data[31],
           data[32], data[33], data[34], data[35], data[36], data[37], data[38], data[39]
       );

       log_event.insert("ipv6_src", src_addr);
       log_event.insert("ipv6_dst", dst_addr);
   }

   /// Parse TCP header
   fn parse_tcp_header(&self, data: &[u8], log_event: &mut LogEvent) {
       if data.len() < 20 {
           return;
       }

       let src_port = u16::from_be_bytes([data[0], data[1]]);
       let dst_port = u16::from_be_bytes([data[2], data[3]]);
       let sequence = u32::from_be_bytes([data[4], data[5], data[6], data[7]]);
       let acknowledgment = u32::from_be_bytes([data[8], data[9], data[10], data[11]]);
       let header_length = ((data[12] >> 4) & 0x0F) * 4;
       let flags = data[13];
       let window_size = u16::from_be_bytes([data[14], data[15]]);
       let checksum = u16::from_be_bytes([data[16], data[17]]);
       let urgent_pointer = u16::from_be_bytes([data[18], data[19]]);

       log_event.insert("tcp_src_port", src_port);
       log_event.insert("tcp_dst_port", dst_port);
       log_event.insert("tcp_sequence", sequence);
       log_event.insert("tcp_acknowledgment", acknowledgment);
       log_event.insert("tcp_header_length", header_length);
       log_event.insert("tcp_flags", flags);
       log_event.insert("tcp_window_size", window_size);
       log_event.insert("tcp_checksum", checksum);
       log_event.insert("tcp_urgent_pointer", urgent_pointer);

       // Parse individual TCP flags
       log_event.insert("tcp_flag_urg", (flags & 0x20) != 0);
       log_event.insert("tcp_flag_ack", (flags & 0x10) != 0);
       log_event.insert("tcp_flag_psh", (flags & 0x08) != 0);
       log_event.insert("tcp_flag_rst", (flags & 0x04) != 0);
       log_event.insert("tcp_flag_syn", (flags & 0x02) != 0);
       log_event.insert("tcp_flag_fin", (flags & 0x01) != 0);
   }

   /// Parse UDP header
   fn parse_udp_header(&self, data: &[u8], log_event: &mut LogEvent) {
       if data.len() < 8 {
           return;
       }

       let src_port = u16::from_be_bytes([data[0], data[1]]);
       let dst_port = u16::from_be_bytes([data[2], data[3]]);
       let length = u16::from_be_bytes([data[4], data[5]]);
       let checksum = u16::from_be_bytes([data[6], data[7]]);

       log_event.insert("udp_src_port", src_port);
       log_event.insert("udp_dst_port", dst_port);
       log_event.insert("udp_length", length);
       log_event.insert("udp_checksum", checksum);
   }

   /// Parse ICMP header
   fn parse_icmp_header(&self, data: &[u8], log_event: &mut LogEvent) {
       if data.len() < 8 {
           return;
       }

       let icmp_type = data[0];
       let icmp_code = data[1];
       let checksum = u16::from_be_bytes([data[2], data[3]]);
       let rest_of_header = u32::from_be_bytes([data[4], data[5], data[6], data[7]]);

       log_event.insert("icmp_type", icmp_type);
       log_event.insert("icmp_code", icmp_code);
       log_event.insert("icmp_checksum", checksum);
       log_event.insert("icmp_rest_of_header", rest_of_header);

       // Common ICMP types
       let icmp_type_name = match icmp_type {
           0 => "Echo Reply",
           3 => "Destination Unreachable",
           4 => "Source Quench",
           5 => "Redirect",
           8 => "Echo Request",
           11 => "Time Exceeded",
           12 => "Parameter Problem",
           13 => "Timestamp Request",
           14 => "Timestamp Reply",
           _ => "Other",
       };
       log_event.insert("icmp_type_name", icmp_type_name);
   }
}

/// sFlow specific events
#[derive(Debug)]
pub struct SflowPacketProcessed {
   pub peer_addr: SocketAddr,
   pub flow_samples: usize,
   pub counter_samples: usize,
   pub unknown_samples: usize,
   pub event_count: usize,
}

impl vector_lib::internal_event::InternalEvent for SflowPacketProcessed {
   fn emit(self) {
       debug!(
           message = "sFlow packet processed",
           peer_addr = %self.peer_addr,
           flow_samples = self.flow_samples,
           counter_samples = self.counter_samples,
           unknown_samples = self.unknown_samples,
           event_count = self.event_count,
       );

       if self.unknown_samples > 0 {
           warn!(
               message = "sFlow packet contained unknown samples",
               peer_addr = %self.peer_addr,
               unknown_samples = self.unknown_samples,
           );
       }
   }
}

#[cfg(test)]
mod tests {
   use super::*;
   use crate::sources::netflow::NetflowConfig;
   use crate::sources::netflow::fields::FieldParser;
   use base64::Engine;
   use std::net::{IpAddr, Ipv4Addr};

   fn test_peer_addr() -> SocketAddr {
       SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 100)), 6343)
   }

   fn create_sflow_header(num_samples: u32) -> Vec<u8> {
       let mut data = vec![0u8; 28];
       data[0..4].copy_from_slice(&5u32.to_be_bytes()); // version
       data[4..8].copy_from_slice(&1u32.to_be_bytes()); // agent_address_type (IPv4)
       data[8..12].copy_from_slice(&0xC0A80101u32.to_be_bytes()); // agent_address (192.168.1.1)
       data[12..16].copy_from_slice(&0u32.to_be_bytes()); // sub_agent_id
       data[16..20].copy_from_slice(&12345u32.to_be_bytes()); // sequence_number
       data[20..24].copy_from_slice(&54321u32.to_be_bytes()); // sys_uptime
       data[24..28].copy_from_slice(&num_samples.to_be_bytes()); // num_samples
       data
   }

   #[test]
   fn test_sflow_header_parsing() {
       let data = create_sflow_header(1);
       let header = SflowHeader::from_bytes(&data).unwrap();

       assert_eq!(header.version, 5);
       assert_eq!(header.agent_address_type, 1);
       assert_eq!(header.agent_address_string(), "192.168.1.1");
       assert_eq!(header.sub_agent_id, 0);
       assert_eq!(header.sequence_number, 12345);
       assert_eq!(header.sys_uptime, 54321);
       assert_eq!(header.num_samples, 1);
   }

   #[test]
   fn test_invalid_sflow_header() {
       // Too short
       let short_data = vec![0u8; 10];
       assert!(SflowHeader::from_bytes(&short_data).is_err());

       // Wrong version
       let mut wrong_version = create_sflow_header(1);
       wrong_version[0..4].copy_from_slice(&4u32.to_be_bytes());
       assert!(SflowHeader::from_bytes(&wrong_version).is_err());

       // Too many samples
       let mut too_many = create_sflow_header(1);
       too_many[24..28].copy_from_slice(&2000u32.to_be_bytes());
       assert!(SflowHeader::from_bytes(&too_many).is_err());

       // Unknown address type
       let mut unknown_addr = create_sflow_header(1);
       unknown_addr[4..8].copy_from_slice(&99u32.to_be_bytes());
       assert!(SflowHeader::from_bytes(&unknown_addr).is_err());
   }

   #[test]
   fn test_can_parse() {
       // Valid sFlow
       let packet = create_sflow_header(1);
       assert!(SflowParser::can_parse(&packet));

       // Invalid version
       let mut invalid_version = packet.clone();
       invalid_version[0..4].copy_from_slice(&4u32.to_be_bytes());
       assert!(!SflowParser::can_parse(&invalid_version));

       // Too short
       let short_packet = vec![0u8; 3];
       assert!(!SflowParser::can_parse(&short_packet));

       // Invalid address type
       let mut invalid_addr = packet.clone();
       invalid_addr[4..8].copy_from_slice(&99u32.to_be_bytes());
       assert!(!SflowParser::can_parse(&invalid_addr));
   }

   #[test]
   fn test_sample_header_parsing() {
       let mut data = vec![0u8; 8];
       data[0..4].copy_from_slice(&1u32.to_be_bytes()); // flow sample
       data[4..8].copy_from_slice(&32u32.to_be_bytes()); // length

       let header = SampleHeader::from_bytes(&data).unwrap();
       assert_eq!(header.sample_type, SampleType::FlowSample);
       assert_eq!(header.sample_length, 32);

       // Test other sample types
       data[0..4].copy_from_slice(&2u32.to_be_bytes()); // counter sample
       let header = SampleHeader::from_bytes(&data).unwrap();
       assert_eq!(header.sample_type, SampleType::CounterSample);

       // Invalid sample type
       data[0..4].copy_from_slice(&99u32.to_be_bytes());
       assert!(SampleHeader::from_bytes(&data).is_err());

       // Invalid length
       data[0..4].copy_from_slice(&1u32.to_be_bytes());
       data[4..8].copy_from_slice(&3u32.to_be_bytes()); // length < 8
       assert!(SampleHeader::from_bytes(&data).is_err());
   }

   #[test]
   fn test_sample_type_conversion() {
       assert_eq!(SampleType::from_u32(1), Some(SampleType::FlowSample));
       assert_eq!(SampleType::from_u32(2), Some(SampleType::CounterSample));
       assert_eq!(SampleType::from_u32(3), Some(SampleType::ExpandedFlowSample));
       assert_eq!(SampleType::from_u32(4), Some(SampleType::ExpandedCounterSample));
       assert_eq!(SampleType::from_u32(99), None);

       assert_eq!(SampleType::FlowSample.as_str(), "flow_sample");
       assert_eq!(SampleType::CounterSample.as_str(), "counter_sample");
   }

   #[test]
   fn test_ipv6_agent_address() {
       let mut data = vec![0u8; 40];
       data[0..4].copy_from_slice(&5u32.to_be_bytes()); // version
       data[4..8].copy_from_slice(&2u32.to_be_bytes()); // agent_address_type (IPv6)
       
       // IPv6 address: 2001:db8::1
       data[8..24].copy_from_slice(&[
           0x20, 0x01, 0x0d, 0xb8, 0x00, 0x00, 0x00, 0x00,
           0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01
       ]);
       
       data[24..28].copy_from_slice(&0u32.to_be_bytes()); // sub_agent_id
       data[28..32].copy_from_slice(&12345u32.to_be_bytes()); // sequence_number
       data[32..36].copy_from_slice(&54321u32.to_be_bytes()); // sys_uptime
       data[36..40].copy_from_slice(&1u32.to_be_bytes()); // num_samples

       let header = SflowHeader::from_bytes(&data).unwrap();
       assert_eq!(header.agent_address_type, 2);
       assert_eq!(header.agent_address_string(), "2001:0db8:0000:0000:0000:0000:0000:0001");
       assert_eq!(header.header_size(), 40);
   }

   #[test]
   fn test_header_to_log_event() {
       let data = create_sflow_header(2);
       let header = SflowHeader::from_bytes(&data).unwrap();
       let log_event = header.to_log_event();

       assert_eq!(log_event.get("flow_type").unwrap().as_str().unwrap(), "sflow");
       assert_eq!(log_event.get("version").unwrap().as_integer().unwrap(), 5);
       assert_eq!(log_event.get("agent_address_type").unwrap().as_integer().unwrap(), 1);
       assert_eq!(log_event.get("agent_address").unwrap().as_str().unwrap(), "192.168.1.1");
       assert_eq!(log_event.get("sequence_number").unwrap().as_integer().unwrap(), 12345);
       assert_eq!(log_event.get("num_samples").unwrap().as_integer().unwrap(), 2);
   }

   #[test]
       fn test_basic_packet_parsing() {
        let config = NetflowConfig::default();
        let _field_parser = FieldParser::new(&config);
        let parser = SflowParser::new();

       let packet = create_sflow_header(0);
       let events = parser.parse(&packet, test_peer_addr(), false).unwrap();

       // Should get header event for zero samples
       assert_eq!(events.len(), 1);
       
       if let Event::Log(log) = &events[0] {
           assert_eq!(log.get("flow_type").unwrap().as_str().unwrap(), "sflow");
           assert_eq!(log.get("num_samples").unwrap().as_integer().unwrap(), 0);
       }
   }

       #[test]
    fn test_flow_sample_parsing() {
        let config = NetflowConfig::default();
        let _field_parser = FieldParser::new(&config);
        let parser = SflowParser::new();

       // Create sFlow packet with flow sample
       let mut packet = create_sflow_header(1);
       
       // Add flow sample
       packet.extend_from_slice(&1u32.to_be_bytes()); // sample_type (flow)
       packet.extend_from_slice(&36u32.to_be_bytes()); // sample_length (8 header + 28 data)
       packet.extend_from_slice(&100u32.to_be_bytes()); // sequence_number
       packet.extend_from_slice(&0x01000001u32.to_be_bytes()); // source_id (type=1, index=1)
       packet.extend_from_slice(&1000u32.to_be_bytes()); // sampling_rate
       packet.extend_from_slice(&5000u32.to_be_bytes()); // sample_pool
       packet.extend_from_slice(&0u32.to_be_bytes()); // drops
       packet.extend_from_slice(&1u32.to_be_bytes()); // input_interface
       packet.extend_from_slice(&2u32.to_be_bytes()); // output_interface
       packet.extend_from_slice(&0u32.to_be_bytes()); // num_flow_records

       let events = parser.parse(&packet, test_peer_addr(), false).unwrap();

       assert_eq!(events.len(), 1);
       
       if let Event::Log(log) = &events[0] {
           assert_eq!(log.get("flow_type").unwrap().as_str().unwrap(), "sflow_flow_sample");
           assert_eq!(log.get("sample_index").unwrap().as_integer().unwrap(), 0);
           assert_eq!(log.get("sequence_number").unwrap().as_integer().unwrap(), 100);
           assert_eq!(log.get("source_id_type").unwrap().as_integer().unwrap(), 1);
           assert_eq!(log.get("source_id_index").unwrap().as_integer().unwrap(), 1);
           assert_eq!(log.get("sampling_rate").unwrap().as_integer().unwrap(), 1000);
           assert_eq!(log.get("sample_pool").unwrap().as_integer().unwrap(), 5000);
           assert_eq!(log.get("input_interface").unwrap().as_integer().unwrap(), 1);
           assert_eq!(log.get("output_interface").unwrap().as_integer().unwrap(), 2);
       }
   }

   #[test]
   fn test_counter_sample_parsing() {
       let config = NetflowConfig::default();
       let _field_parser = FieldParser::new(&config);
       let parser = SflowParser::new();

       // Create sFlow packet with counter sample
       let mut packet = create_sflow_header(1);
       
       // Add counter sample
       packet.extend_from_slice(&2u32.to_be_bytes()); // sample_type (counter)
       packet.extend_from_slice(&20u32.to_be_bytes()); // sample_length (8 header + 12 data)
       packet.extend_from_slice(&200u32.to_be_bytes()); // sequence_number
       packet.extend_from_slice(&0x01000002u32.to_be_bytes()); // source_id (type=1, index=2)
       packet.extend_from_slice(&0u32.to_be_bytes()); // num_counter_records

       let events = parser.parse(&packet, test_peer_addr(), false).unwrap();

       assert_eq!(events.len(), 1);
       
       if let Event::Log(log) = &events[0] {
           assert_eq!(log.get("flow_type").unwrap().as_str().unwrap(), "sflow_counter_sample");
           assert_eq!(log.get("sample_index").unwrap().as_integer().unwrap(), 0);
           assert_eq!(log.get("sequence_number").unwrap().as_integer().unwrap(), 200);
           assert_eq!(log.get("source_id_type").unwrap().as_integer().unwrap(), 1);
           assert_eq!(log.get("source_id_index").unwrap().as_integer().unwrap(), 2);
           assert_eq!(log.get("num_counter_records").unwrap().as_integer().unwrap(), 0);
       }
   }

   #[test]
   fn test_raw_data_inclusion() {
       let config = NetflowConfig::default();
       let _field_parser = FieldParser::new(&config);
       let parser = SflowParser::new();

       let packet = create_sflow_header(0);

       // Test with raw data inclusion
       let events = parser.parse(&packet, test_peer_addr(), true).unwrap();
       assert!(!events.is_empty());
       
       if let Event::Log(log) = &events[0] {
           assert!(log.get("raw_data").is_some());
           let raw_data = log.get("raw_data").unwrap().as_str().unwrap();
           
                       // Should be valid base64
            assert!(base64::engine::general_purpose::STANDARD.decode(raw_data.as_bytes()).is_ok());
       }

       // Test without raw data inclusion
       let events = parser.parse(&packet, test_peer_addr(), false).unwrap();
       assert!(!events.is_empty());
       
       if let Event::Log(log) = &events[0] {
           assert!(log.get("raw_data").is_none());
       }
   }

   #[test]
   fn test_malformed_sample_handling() {
       let config = NetflowConfig::default();
       let _field_parser = FieldParser::new(&config);
       let parser = SflowParser::new();

       // Create packet with malformed sample
       let mut packet = create_sflow_header(1);
       
       // Add truncated sample
       packet.extend_from_slice(&1u32.to_be_bytes()); // sample_type
       packet.extend_from_slice(&100u32.to_be_bytes()); // claimed length (too large)
       packet.extend_from_slice(&[0u8; 10]); // only 10 bytes of data

       let events = parser.parse(&packet, test_peer_addr(), false).unwrap();

       // Should handle gracefully and return header event
       assert!(!events.is_empty());
   }

   #[test]
   fn test_ethernet_header_parsing() {
       let config = NetflowConfig::default();
       let _field_parser = FieldParser::new(&config);
       let parser = SflowParser::new();

       let eth_data = vec![
           0x00, 0x1B, 0x21, 0x3C, 0x4D, 0x5E, // dst MAC
           0x00, 0x1A, 0x2B, 0x3C, 0x4D, 0x5F, // src MAC
           0x08, 0x00, // ethertype (IPv4)
       ];

       let mut log_event = LogEvent::default();
       parser.parse_ethernet_header(&eth_data, &mut log_event);

       assert_eq!(log_event.get("eth_dst").unwrap().as_str().unwrap(), "00:1b:21:3c:4d:5e");
       assert_eq!(log_event.get("eth_src").unwrap().as_str().unwrap(), "00:1a:2b:3c:4d:5f");
       assert_eq!(log_event.get("eth_type").unwrap().as_integer().unwrap(), 0x0800);
   }

   #[test]
   fn test_ipv4_header_parsing() {
       let config = NetflowConfig::default();
       let _field_parser = FieldParser::new(&config);
       let parser = SflowParser::new();

       let ipv4_data = vec![
           0x45, // version=4, ihl=5
           0x00, // tos
           0x00, 0x3C, // total_length
           0x1C, 0x46, // identification
           0x40, 0x00, // flags=2, fragment_offset=0
           0x40, // ttl=64
           0x06, // protocol=TCP
           0xB1, 0xE6, // checksum
           0xC0, 0xA8, 0x01, 0x01, // src: 192.168.1.1
           0x08, 0x08, 0x08, 0x08, // dst: 8.8.8.8
       ];

       let mut log_event = LogEvent::default();
       parser.parse_ipv4_header(&ipv4_data, &mut log_event);

       assert_eq!(log_event.get("ip_version").unwrap().as_integer().unwrap(), 4);
       assert_eq!(log_event.get("ip_ihl").unwrap().as_integer().unwrap(), 5);
       assert_eq!(log_event.get("ip_protocol").unwrap().as_integer().unwrap(), 6);
       assert_eq!(log_event.get("ip_ttl").unwrap().as_integer().unwrap(), 64);
       assert_eq!(log_event.get("ip_src").unwrap().as_str().unwrap(), "192.168.1.1");
       assert_eq!(log_event.get("ip_dst").unwrap().as_str().unwrap(), "8.8.8.8");
   }

   #[test]
   fn test_tcp_header_parsing() {
       let config = NetflowConfig::default();
       let _field_parser = FieldParser::new(&config);
       let parser = SflowParser::new();

       let tcp_data = vec![
           0x00, 0x50, // src_port: 80
           0x01, 0xBB, // dst_port: 443
           0x00, 0x00, 0x00, 0x01, // sequence
           0x00, 0x00, 0x00, 0x02, // acknowledgment
           0x50, // header_length=20
           0x18, // flags: ACK+PSH
           0x20, 0x00, // window_size
           0x00, 0x00, // checksum
           0x00, 0x00, // urgent_pointer
       ];

       let mut log_event = LogEvent::default();
       parser.parse_tcp_header(&tcp_data, &mut log_event);

       assert_eq!(log_event.get("tcp_src_port").unwrap().as_integer().unwrap(), 80);
       assert_eq!(log_event.get("tcp_dst_port").unwrap().as_integer().unwrap(), 443);
       assert_eq!(log_event.get("tcp_flags").unwrap().as_integer().unwrap(), 0x18);
       assert_eq!(log_event.get("tcp_flag_ack").unwrap().as_boolean().unwrap(), true);
       assert_eq!(log_event.get("tcp_flag_psh").unwrap().as_boolean().unwrap(), true);
       assert_eq!(log_event.get("tcp_flag_syn").unwrap().as_boolean().unwrap(), false);
   }

   #[test]
   fn test_udp_header_parsing() {
       let config = NetflowConfig::default();
       let _field_parser = FieldParser::new(&config);
       let parser = SflowParser::new();

       let udp_data = vec![
           0x00, 0x35, // src_port: 53 (DNS)
           0xC0, 0x00, // dst_port: 49152
           0x00, 0x20, // length: 32
           0x00, 0x00, // checksum
       ];

       let mut log_event = LogEvent::default();
       parser.parse_udp_header(&udp_data, &mut log_event);

       assert_eq!(log_event.get("udp_src_port").unwrap().as_integer().unwrap(), 53);
       assert_eq!(log_event.get("udp_dst_port").unwrap().as_integer().unwrap(), 49152);
       assert_eq!(log_event.get("udp_length").unwrap().as_integer().unwrap(), 32);
   }

   #[test]
   fn test_icmp_header_parsing() {
       let config = NetflowConfig::default();
       let _field_parser = FieldParser::new(&config);
       let parser = SflowParser::new();

       let icmp_data = vec![
           0x08, // type: Echo Request
           0x00, // code
           0x00, 0x00, // checksum
           0x00, 0x01, 0x00, 0x02, // rest of header
       ];

       let mut log_event = LogEvent::default();
       parser.parse_icmp_header(&icmp_data, &mut log_event);

       assert_eq!(log_event.get("icmp_type").unwrap().as_integer().unwrap(), 8);
       assert_eq!(log_event.get("icmp_code").unwrap().as_integer().unwrap(), 0);
       assert_eq!(log_event.get("icmp_type_name").unwrap().as_str().unwrap(), "Echo Request");
   }

   #[test]
   fn test_interface_counters_parsing() {
       let config = NetflowConfig::default();
       let _field_parser = FieldParser::new(&config);
       let parser = SflowParser::new();

       let mut counter_data = vec![0u8; 88];
       
       // Set some test values
       counter_data[0..4].copy_from_slice(&1u32.to_be_bytes()); // if_index
       counter_data[4..8].copy_from_slice(&6u32.to_be_bytes()); // if_type (ethernet)
       counter_data[8..16].copy_from_slice(&1000000000u64.to_be_bytes()); // if_speed (1 Gbps)
       counter_data[24..32].copy_from_slice(&1000000u64.to_be_bytes()); // if_in_octets
       counter_data[32..36].copy_from_slice(&10000u32.to_be_bytes()); // if_in_ucast_pkts
       counter_data[56..64].copy_from_slice(&2000000u64.to_be_bytes()); // if_out_octets
       counter_data[64..68].copy_from_slice(&20000u32.to_be_bytes()); // if_out_ucast_pkts

       let mut log_event = LogEvent::default();
       parser.parse_interface_counters(&counter_data, &mut log_event);

       assert_eq!(log_event.get("if_index").unwrap().as_integer().unwrap(), 1);
       assert_eq!(log_event.get("if_type").unwrap().as_integer().unwrap(), 6);
       assert_eq!(log_event.get("if_speed").unwrap().as_integer().unwrap(), 1000000000);
       assert_eq!(log_event.get("if_in_octets").unwrap().as_integer().unwrap(), 1000000);
       assert_eq!(log_event.get("if_out_octets").unwrap().as_integer().unwrap(), 2000000);
       
       // Check calculated utilization
       assert!(log_event.get("if_utilization_percent").is_some());
       let utilization = log_event.get("if_utilization_percent").unwrap().as_float().unwrap();
               assert!(utilization > ordered_float::NotNan::new(0.0).unwrap());
   }

   #[test]
   fn test_ethernet_counters_parsing() {
       let config = NetflowConfig::default();
       let _field_parser = FieldParser::new(&config);
       let parser = SflowParser::new();

       let mut counter_data = vec![0u8; 52];
       
       // Set some test values
       counter_data[0..4].copy_from_slice(&10u32.to_be_bytes()); // alignment_errors
       counter_data[4..8].copy_from_slice(&5u32.to_be_bytes()); // fcs_errors
       counter_data[24..28].copy_from_slice(&2u32.to_be_bytes()); // late_collisions

       let mut log_event = LogEvent::default();
       parser.parse_ethernet_counters(&counter_data, &mut log_event);

       assert_eq!(log_event.get("dot3_stats_alignment_errors").unwrap().as_integer().unwrap(), 10);
       assert_eq!(log_event.get("dot3_stats_fcs_errors").unwrap().as_integer().unwrap(), 5);
       assert_eq!(log_event.get("dot3_stats_late_collisions").unwrap().as_integer().unwrap(), 2);
   }

   #[test]
   fn test_multiple_samples() {
       let config = NetflowConfig::default();
       let _field_parser = FieldParser::new(&config);
       let parser = SflowParser::new();

       // Create packet with multiple samples
       let mut packet = create_sflow_header(2);
       
       // First sample (flow)
       packet.extend_from_slice(&1u32.to_be_bytes()); // sample_type
       packet.extend_from_slice(&44u32.to_be_bytes()); // sample_length
       packet.extend([0u8; 36]); // sample data
       
       // Second sample (counter)
       packet.extend_from_slice(&2u32.to_be_bytes()); // sample_type
       packet.extend_from_slice(&28u32.to_be_bytes()); // sample_length
       packet.extend([0u8; 20]); // sample data

       let events = parser.parse(&packet, test_peer_addr(), false).unwrap();

       // Should get 2 sample events
       assert_eq!(events.len(), 2);
       
       let flow_event = events.iter().find(|e| {
           if let Event::Log(log) = e {
               log.get("flow_type").unwrap().as_str().unwrap().contains("flow")
           } else {
               false
           }
       });
       assert!(flow_event.is_some());
       
       let counter_event = events.iter().find(|e| {
           if let Event::Log(log) = e {
               log.get("flow_type").unwrap().as_str().unwrap().contains("counter")
           } else {
               false
           }
       });
       assert!(counter_event.is_some());
   }

   #[test]
   fn test_unknown_sample_type() {
       let config = NetflowConfig::default();
       let _field_parser = FieldParser::new(&config);
       let parser = SflowParser::new();

       let mut packet = create_sflow_header(1);
       
       // Add unknown sample type
       packet.extend_from_slice(&99u32.to_be_bytes()); // unknown sample_type
       packet.extend_from_slice(&16u32.to_be_bytes()); // sample_length
       packet.extend([0u8; 8]); // minimal sample data

       let events = parser.parse(&packet, test_peer_addr(), false).unwrap();

       // Should return header event due to unknown sample
       assert_eq!(events.len(), 1);
       
       if let Event::Log(log) = &events[0] {
           assert_eq!(log.get("flow_type").unwrap().as_str().unwrap(), "sflow");
           assert_eq!(log.get("unknown_samples").unwrap().as_integer().unwrap(), 1);
       }
   }
}