//! Field parsing and enterprise field definitions for NetFlow/IPFIX.
//!
//! This module provides unified field parsing logic with support for:
//! - Standard IPFIX fields
//! - NetFlow v9 fields  
//! - Enterprise-specific fields (HPE Aruba, Cisco, etc.)
//! - Custom user-defined enterprise fields

use crate::sources::netflow::config::{FieldType, NetflowConfig};
use crate::sources::netflow::events::*;
use crate::sources::netflow::templates::TemplateField;

use std::collections::HashMap;
use std::sync::OnceLock;
use base64::Engine;
use vector_lib::event::LogEvent;

use vector_lib::event::Value;

/// Field information including name and data type.
#[derive(Debug, Clone)]
pub struct FieldInfo {
    pub name: &'static str,
    pub data_type: DataType,
    pub description: &'static str,
}

/// Supported data types for field parsing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DataType {
    UInt8,
    UInt16,
    UInt32,
    UInt64,
    Int8,
    Int16,
    Int32,
    Int64,
    Float32,
    Float64,
    Boolean,
    MacAddress,
    Ipv4Address,
    Ipv6Address,
    DateTimeSeconds,
    DateTimeMilliseconds,
    DateTimeMicroseconds,
    DateTimeNanoseconds,
    String,
    Binary,
}

impl DataType {
    /// Parse field data into a Value for insertion into LogEvent.
    pub fn parse(&self, data: &[u8], max_length: usize) -> Result<Value, String> {
        match self {
            DataType::UInt8 => {
                if data.len() >= 1 {
                    Ok(Value::Integer(data[0] as i64))
                } else {
                    Err("Insufficient data for UInt8".to_string())
                }
            }
            DataType::UInt16 => {
                if data.len() >= 2 {
                    let value = u16::from_be_bytes([data[0], data[1]]);
                    Ok(Value::Integer(value as i64))
                } else {
                    Err("Insufficient data for UInt16".to_string())
                }
            }
            DataType::UInt32 => {
                if data.len() >= 4 {
                    let value = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);
                    Ok(Value::Integer(value as i64))
                } else {
                    Err("Insufficient data for UInt32".to_string())
                }
            }
            DataType::UInt64 => {
                if data.len() >= 8 {
                    let value = u64::from_be_bytes([
                        data[0], data[1], data[2], data[3],
                        data[4], data[5], data[6], data[7]
                    ]);
                    // Convert to i64, clamping if necessary
                    if value <= i64::MAX as u64 {
                        Ok(Value::Integer(value as i64))
                    } else {
                        Ok(Value::Integer(i64::MAX))
                    }
                } else {
                    Err("Insufficient data for UInt64".to_string())
                }
            }
            DataType::Int32 => {
                if data.len() >= 4 {
                    let value = i32::from_be_bytes([data[0], data[1], data[2], data[3]]);
                    Ok(Value::Integer(value as i64))
                } else {
                    Err("Insufficient data for Int32".to_string())
                }
            }
            DataType::Float32 => {
                if data.len() >= 4 {
                    let bits = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);
                    let value = f32::from_bits(bits);
                    Ok(Value::Float(ordered_float::NotNan::new(value as f64).unwrap()))
                } else {
                    Err("Insufficient data for Float32".to_string())
                }
            }
            DataType::Float64 => {
                if data.len() >= 8 {
                    let bits = u64::from_be_bytes([
                        data[0], data[1], data[2], data[3],
                        data[4], data[5], data[6], data[7]
                    ]);
                    let value = f64::from_bits(bits);
                    Ok(Value::Float(ordered_float::NotNan::new(value).unwrap()))
                } else {
                    Err("Insufficient data for Float64".to_string())
                }
            }
            DataType::Boolean => {
                if data.len() >= 1 {
                    Ok(Value::Boolean(data[0] != 0))
                } else {
                    Err("Insufficient data for Boolean".to_string())
                }
            }
            DataType::MacAddress => {
                if data.len() >= 6 {
                    let mac = format!(
                        "{:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
                        data[0], data[1], data[2], data[3], data[4], data[5]
                    );
                    Ok(Value::Bytes(mac.into()))
                } else {
                    Err("Insufficient data for MAC address".to_string())
                }
            }
            DataType::Ipv4Address => {
                if data.len() >= 4 {
                    let addr = format!(
                        "{}.{}.{}.{}",
                        data[0], data[1], data[2], data[3]
                    );
                    Ok(Value::Bytes(addr.into()))
                } else {
                    Err("Insufficient data for IPv4 address".to_string())
                }
            }
            DataType::Ipv6Address => {
                if data.len() >= 16 {
                    let addr = format!(
                        "{:02x}{:02x}:{:02x}{:02x}:{:02x}{:02x}:{:02x}{:02x}:{:02x}{:02x}:{:02x}{:02x}:{:02x}{:02x}:{:02x}{:02x}",
                        data[0], data[1], data[2], data[3], data[4], data[5], data[6], data[7],
                        data[8], data[9], data[10], data[11], data[12], data[13], data[14], data[15]
                    );
                    Ok(Value::Bytes(addr.into()))
                } else {
                    Err("Insufficient data for IPv6 address".to_string())
                }
            }
            DataType::DateTimeSeconds => {
                if data.len() >= 4 {
                    let timestamp = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);
                    Ok(Value::Integer(timestamp as i64))
                } else {
                    Err("Insufficient data for DateTime seconds".to_string())
                }
            }
            DataType::DateTimeMilliseconds => {
                if data.len() >= 8 {
                    let timestamp = u64::from_be_bytes([
                        data[0], data[1], data[2], data[3],
                        data[4], data[5], data[6], data[7]
                    ]);
                    Ok(Value::Integer(timestamp as i64))
                } else {
                    Err("Insufficient data for DateTime milliseconds".to_string())
                }
            }
            DataType::String => {
                if data.is_empty() {
                    Ok(Value::Bytes("".into()))
                } else {
                    match std::str::from_utf8(data) {
                        Ok(s) => {
                            let clean_str = s.trim_matches('\0').trim();
                            let truncated = if clean_str.len() > max_length {
                                format!("{}...", &clean_str[..max_length.saturating_sub(3)])
                            } else {
                                clean_str.to_string()
                            };
                            Ok(Value::Bytes(truncated.into()))
                        }
                        Err(_) => {
                            // Fallback to binary if not valid UTF-8
                            let encoded = base64::engine::general_purpose::STANDARD.encode(data);
                            let truncated = if encoded.len() > max_length {
                                format!("{}...", &encoded[..max_length.saturating_sub(3)])
                            } else {
                                encoded
                            };
                            Ok(Value::Bytes(truncated.into()))
                        }
                    }
                }
            }
            DataType::Binary => {
                if data.is_empty() {
                    Ok(Value::Bytes("".into()))
                } else {
                    let encoded = base64::engine::general_purpose::STANDARD.encode(data);
                    let truncated = if encoded.len() > max_length {
                        format!("{}...", &encoded[..max_length.saturating_sub(3)])
                    } else {
                        encoded
                    };
                    Ok(Value::Bytes(truncated.into()))
                }
            }
            // Add other integer types
            DataType::Int8 => {
                if data.len() >= 1 {
                    let value = data[0] as i8;
                    Ok(Value::Integer(value as i64))
                } else {
                    Err("Insufficient data for Int8".to_string())
                }
            }
            DataType::Int16 => {
                if data.len() >= 2 {
                    let value = i16::from_be_bytes([data[0], data[1]]);
                    Ok(Value::Integer(value as i64))
                } else {
                    Err("Insufficient data for Int16".to_string())
                }
            }
            DataType::Int64 => {
                if data.len() >= 8 {
                    let value = i64::from_be_bytes([
                        data[0], data[1], data[2], data[3],
                        data[4], data[5], data[6], data[7]
                    ]);
                    Ok(Value::Integer(value))
                } else {
                    Err("Insufficient data for Int64".to_string())
                }
            }
            DataType::DateTimeMicroseconds | DataType::DateTimeNanoseconds => {
                // Handle as UInt64 timestamp
                if data.len() >= 8 {
                    let timestamp = u64::from_be_bytes([
                        data[0], data[1], data[2], data[3],
                        data[4], data[5], data[6], data[7]
                    ]);
                    Ok(Value::Integer(timestamp as i64))
                } else {
                    Err("Insufficient data for DateTime".to_string())
                }
            }
        }
    }
}

impl From<FieldType> for DataType {
    fn from(field_type: FieldType) -> Self {
        match field_type {
            FieldType::Uint8 => DataType::UInt8,
            FieldType::Uint16 => DataType::UInt16,
            FieldType::Uint32 => DataType::UInt32,
            FieldType::Uint64 => DataType::UInt64,
            FieldType::Ipv4Address => DataType::Ipv4Address,
            FieldType::Ipv6Address => DataType::Ipv6Address,
            FieldType::MacAddress => DataType::MacAddress,
            FieldType::String => DataType::String,
            FieldType::Binary => DataType::Binary,
            FieldType::Boolean => DataType::Boolean,
            FieldType::Float32 => DataType::Float32,
            FieldType::Float64 => DataType::Float64,
        }
    }
}

/// Field parser that handles all field types and enterprise extensions.
#[derive(Clone)]
pub struct FieldParser {
    max_field_length: usize,
    resolve_protocols: bool,
    custom_enterprise_fields: HashMap<(u32, u16), (String, DataType)>, // (enterprise_id, field_id) -> (name, type)
}

impl FieldParser {
    /// Create a new field parser with the given configuration.
    pub fn new(config: &NetflowConfig) -> Self {
        let mut custom_enterprise_fields = HashMap::new();
        
        // Load custom enterprise field mappings from config
        for (key, field_name) in &config.enterprise_fields {
            if let Some((enterprise_str, field_str)) = key.split_once(':') {
                if let (Ok(enterprise_id), Ok(field_id)) = (enterprise_str.parse::<u32>(), field_str.parse::<u16>()) {
                    // Try to infer data type from field name or default to UInt32 for numeric fields
                    let data_type = if field_name.contains("address") || field_name.contains("Address") {
                        DataType::Ipv4Address
                    } else if field_name.contains("port") || field_name.contains("Port") {
                        DataType::UInt16
                    } else if field_name.contains("count") || field_name.contains("Count") || 
                              field_name.contains("number") || field_name.contains("Number") ||
                              field_name.contains("id") || field_name.contains("Id") {
                        DataType::UInt32
                    } else {
                        DataType::UInt32 // Default to UInt32 for most custom fields
                    };
                    
                    custom_enterprise_fields.insert(
                        (enterprise_id, field_id),
                        (field_name.clone(), data_type)
                    );
                }
            }
        }

        Self {
            max_field_length: config.max_field_length,
            resolve_protocols: true, // Always resolve protocols for better readability
            custom_enterprise_fields,
        }
    }

    /// Parse a field and insert it into the log event.
    pub fn parse_field(&self, field: &TemplateField, data: &[u8], log_event: &mut LogEvent) {
        let field_info = self.get_field_info(field);
        
        match field_info.data_type.parse(data, self.max_field_length) {
            Ok(value) => {
                log_event.insert(field_info.name, value.clone());
                
                // Add protocol name resolution for protocol fields
                if self.resolve_protocols && field_info.name == "protocolIdentifier" {
                    if let Value::Integer(protocol_num) = &value {
                        let protocol_name = get_protocol_name(*protocol_num as u8);
                        log_event.insert("protocolName", Value::Bytes(protocol_name.into()));
                    }
                }
            }
            Err(error) => {
                emit!(NetflowFieldParseError {
                    error: &error,
                    field_type: field.field_type,
                    template_id: 0, // Would need to pass this through
                    peer_addr: std::net::SocketAddr::new(
                        std::net::IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED), 
                        0
                    ),
                });
                
                // Insert raw data as fallback
                let encoded = base64::engine::general_purpose::STANDARD.encode(data);
                let truncated = if encoded.len() > self.max_field_length {
                    format!("{}...", &encoded[..self.max_field_length.saturating_sub(3)])
                } else {
                    encoded
                };
                log_event.insert(field_info.name, Value::Bytes(truncated.into()));
            }
        }
    }

    /// Get field information for a template field.
    fn get_field_info(&self, field: &TemplateField) -> FieldInfo {
        match field.enterprise_number {
            Some(enterprise_id) => {
                // Check custom enterprise fields first
                if let Some((name, data_type)) = self.custom_enterprise_fields.get(&(enterprise_id, field.field_type)) {
                    return FieldInfo {
                        name: Box::leak(name.clone().into_boxed_str()),
                        data_type: data_type.clone(),
                        description: "Custom enterprise field",
                    };
                }
                
                // Check known enterprise fields
                match enterprise_id {
                    23867 => self.get_hpe_aruba_field(field.field_type),
                    9 => self.get_cisco_field(field.field_type),
                    2636 => self.get_juniper_field(field.field_type),
                    _ => self.get_unknown_enterprise_field(enterprise_id, field.field_type),
                }
            }
            None => self.get_standard_field(field.field_type),
        }
    }

    /// Get standard IPFIX/NetFlow field information.
    fn get_standard_field(&self, field_type: u16) -> FieldInfo {
        STANDARD_FIELDS.get_or_init(init_standard_fields)
            .get(&field_type)
            .cloned()
            .unwrap_or_else(|| FieldInfo {
                name: Box::leak(format!("unknown_field_{}", field_type).into_boxed_str()),
                data_type: DataType::Binary,
                description: "Unknown standard field",
            })
    }

    /// Get HPE Aruba enterprise field information.
    fn get_hpe_aruba_field(&self, field_type: u16) -> FieldInfo {
        HPE_ARUBA_FIELDS.get_or_init(init_hpe_aruba_fields)
            .get(&field_type)
            .cloned()
            .unwrap_or_else(|| FieldInfo {
                name: Box::leak(format!("hpe_aruba_field_{}", field_type).into_boxed_str()),
                data_type: DataType::Binary,
                description: "Unknown HPE Aruba field",
            })
    }

    /// Get Cisco enterprise field information.
    fn get_cisco_field(&self, field_type: u16) -> FieldInfo {
        CISCO_FIELDS.get_or_init(init_cisco_fields)
            .get(&field_type)
            .cloned()
            .unwrap_or_else(|| FieldInfo {
                name: Box::leak(format!("cisco_field_{}", field_type).into_boxed_str()),
                data_type: DataType::Binary,
                description: "Unknown Cisco field",
            })
    }

    /// Get Juniper enterprise field information.
    fn get_juniper_field(&self, field_type: u16) -> FieldInfo {
        JUNIPER_FIELDS.get_or_init(init_juniper_fields)
            .get(&field_type)
            .cloned()
            .unwrap_or_else(|| FieldInfo {
                name: Box::leak(format!("juniper_field_{}", field_type).into_boxed_str()),
                data_type: DataType::Binary,
                description: "Unknown Juniper field",
            })
    }

    /// Get unknown enterprise field information.
    fn get_unknown_enterprise_field(&self, enterprise_id: u32, field_type: u16) -> FieldInfo {
        FieldInfo {
            name: Box::leak(format!("enterprise_{}_{}", enterprise_id, field_type).into_boxed_str()),
            data_type: DataType::Binary,
            description: "Unknown enterprise field",
        }
    }
}

// Static field registries using OnceLock for thread-safe lazy initialization
static STANDARD_FIELDS: OnceLock<HashMap<u16, FieldInfo>> = OnceLock::new();
static HPE_ARUBA_FIELDS: OnceLock<HashMap<u16, FieldInfo>> = OnceLock::new();
static CISCO_FIELDS: OnceLock<HashMap<u16, FieldInfo>> = OnceLock::new();
static JUNIPER_FIELDS: OnceLock<HashMap<u16, FieldInfo>> = OnceLock::new();

/// Initialize standard IPFIX field definitions.
fn init_standard_fields() -> HashMap<u16, FieldInfo> {
    let mut map = HashMap::new();
    
    // Basic flow fields
    map.insert(1, FieldInfo { name: "octetDeltaCount", data_type: DataType::UInt64, description: "Octets in flow" });
    map.insert(2, FieldInfo { name: "packetDeltaCount", data_type: DataType::UInt64, description: "Packets in flow" });
    map.insert(3, FieldInfo { name: "deltaFlowCount", data_type: DataType::UInt64, description: "Number of flows" });
    map.insert(4, FieldInfo { name: "protocolIdentifier", data_type: DataType::UInt8, description: "IP protocol number" });
    map.insert(5, FieldInfo { name: "ipClassOfService", data_type: DataType::UInt8, description: "IP ToS byte" });
    map.insert(6, FieldInfo { name: "tcpControlBits", data_type: DataType::UInt8, description: "TCP flags" });
    map.insert(7, FieldInfo { name: "sourceTransportPort", data_type: DataType::UInt16, description: "Source port" });
    map.insert(8, FieldInfo { name: "sourceIPv4Address", data_type: DataType::Ipv4Address, description: "Source IPv4 address" });
    map.insert(9, FieldInfo { name: "sourceIPv4PrefixLength", data_type: DataType::UInt8, description: "Source subnet mask" });
    map.insert(10, FieldInfo { name: "ingressInterface", data_type: DataType::UInt32, description: "Ingress interface index" });
    map.insert(11, FieldInfo { name: "destinationTransportPort", data_type: DataType::UInt16, description: "Destination port" });
    map.insert(12, FieldInfo { name: "destinationIPv4Address", data_type: DataType::Ipv4Address, description: "Destination IPv4 address" });
    map.insert(13, FieldInfo { name: "destinationIPv4PrefixLength", data_type: DataType::UInt8, description: "Destination subnet mask" });
    map.insert(14, FieldInfo { name: "egressInterface", data_type: DataType::UInt32, description: "Egress interface index" });
    map.insert(15, FieldInfo { name: "ipNextHopIPv4Address", data_type: DataType::Ipv4Address, description: "Next hop IPv4 address" });
    
    // BGP fields
    map.insert(16, FieldInfo { name: "bgpSourceAsNumber", data_type: DataType::UInt32, description: "Source AS number" });
    map.insert(17, FieldInfo { name: "bgpDestinationAsNumber", data_type: DataType::UInt32, description: "Destination AS number" });
    map.insert(18, FieldInfo { name: "bgpNextHopIPv4Address", data_type: DataType::Ipv4Address, description: "BGP next hop IPv4" });
    
    // Timing fields
    map.insert(21, FieldInfo { name: "flowEndSysUpTime", data_type: DataType::UInt32, description: "Flow end system uptime" });
    map.insert(22, FieldInfo { name: "flowStartSysUpTime", data_type: DataType::UInt32, description: "Flow start system uptime" });
    map.insert(150, FieldInfo { name: "flowStartSeconds", data_type: DataType::DateTimeSeconds, description: "Flow start timestamp" });
    map.insert(151, FieldInfo { name: "flowEndSeconds", data_type: DataType::DateTimeSeconds, description: "Flow end timestamp" });
    map.insert(152, FieldInfo { name: "flowStartMilliseconds", data_type: DataType::DateTimeMilliseconds, description: "Flow start milliseconds" });
    map.insert(153, FieldInfo { name: "flowEndMilliseconds", data_type: DataType::DateTimeMilliseconds, description: "Flow end milliseconds" });
    
    // IPv6 fields
    map.insert(27, FieldInfo { name: "sourceIPv6Address", data_type: DataType::Ipv6Address, description: "Source IPv6 address" });
    map.insert(28, FieldInfo { name: "destinationIPv6Address", data_type: DataType::Ipv6Address, description: "Destination IPv6 address" });
    map.insert(29, FieldInfo { name: "sourceIPv6PrefixLength", data_type: DataType::UInt8, description: "Source IPv6 prefix length" });
    map.insert(30, FieldInfo { name: "destinationIPv6PrefixLength", data_type: DataType::UInt8, description: "Destination IPv6 prefix length" });
    map.insert(31, FieldInfo { name: "flowLabelIPv6", data_type: DataType::UInt32, description: "IPv6 flow label" });
    
    // Additional common fields
    map.insert(56, FieldInfo { name: "sourceMacAddress", data_type: DataType::MacAddress, description: "Source MAC address" });
    map.insert(57, FieldInfo { name: "postDestinationMacAddress", data_type: DataType::MacAddress, description: "Post-destination MAC" });
    map.insert(58, FieldInfo { name: "vlanId", data_type: DataType::UInt16, description: "VLAN ID" });
    map.insert(60, FieldInfo { name: "ipVersion", data_type: DataType::UInt8, description: "IP version" });
    map.insert(61, FieldInfo { name: "flowDirection", data_type: DataType::UInt8, description: "Flow direction" });
    
    // Flow record metadata
    map.insert(148, FieldInfo { name: "flowId", data_type: DataType::UInt64, description: "Flow identifier" });
    map.insert(149, FieldInfo { name: "observationDomainId", data_type: DataType::UInt32, description: "Observation domain ID" });
    
    map
}

/// Initialize HPE Aruba enterprise field definitions.
fn init_hpe_aruba_fields() -> HashMap<u16, FieldInfo> {
    let mut map = HashMap::new();
    
    // HPE Aruba EdgeConnect SD-WAN fields
    map.insert(1, FieldInfo { name: "clientIPv4Address", data_type: DataType::Ipv4Address, description: "Client IPv4 address" });
    map.insert(2, FieldInfo { name: "serverIPv4Address", data_type: DataType::Ipv4Address, description: "Server IPv4 address" });
    map.insert(3, FieldInfo { name: "connectionServerOctetDeltaCount", data_type: DataType::UInt64, description: "Server octets" });
    map.insert(4, FieldInfo { name: "connectionServerPacketDeltaCount", data_type: DataType::UInt64, description: "Server packets" });
    map.insert(5, FieldInfo { name: "connectionClientOctetDeltaCount", data_type: DataType::UInt64, description: "Client octets" });
    map.insert(6, FieldInfo { name: "connectionClientPacketDeltaCount", data_type: DataType::UInt64, description: "Client packets" });
    map.insert(7, FieldInfo { name: "connectionInitiator", data_type: DataType::Ipv4Address, description: "Connection initiator" });
    map.insert(8, FieldInfo { name: "applicationHttpHost", data_type: DataType::String, description: "HTTP host header" });
    map.insert(9, FieldInfo { name: "connectionNumberOfConnections", data_type: DataType::UInt8, description: "Number of connections" });
    map.insert(10, FieldInfo { name: "connectionServerResponsesCount", data_type: DataType::UInt8, description: "Server responses count" });
    map.insert(11, FieldInfo { name: "connectionServerResponseDelay", data_type: DataType::UInt32, description: "Server response delay (μs)" });
    map.insert(12, FieldInfo { name: "connectionNetworkToServerDelay", data_type: DataType::UInt32, description: "Network to server delay (μs)" });
    map.insert(13, FieldInfo { name: "connectionNetworkToClientDelay", data_type: DataType::UInt32, description: "Network to client delay (μs)" });
    map.insert(14, FieldInfo { name: "connectionClientPacketRetransmissionCount", data_type: DataType::UInt32, description: "Client retransmissions" });
    map.insert(15, FieldInfo { name: "connectionClientToServerNetworkDelay", data_type: DataType::UInt32, description: "Client to server delay (μs)" });
    map.insert(16, FieldInfo { name: "connectionApplicationDelay", data_type: DataType::UInt32, description: "Application delay (μs)" });
    map.insert(17, FieldInfo { name: "connectionClientToServerResponseDelay", data_type: DataType::UInt32, description: "Client to server response delay (μs)" });
    map.insert(18, FieldInfo { name: "connectionTransactionDuration", data_type: DataType::UInt32, description: "Transaction duration (μs)" });
    map.insert(19, FieldInfo { name: "connectionTransactionDurationMin", data_type: DataType::UInt32, description: "Min transaction duration (μs)" });
    map.insert(20, FieldInfo { name: "connectionTransactionDurationMax", data_type: DataType::UInt32, description: "Max transaction duration (μs)" });
    map.insert(21, FieldInfo { name: "connectionTransactionCompleteCount", data_type: DataType::UInt8, description: "Complete transactions" });
    map.insert(22, FieldInfo { name: "fromZone", data_type: DataType::String, description: "Source zone" });
    map.insert(23, FieldInfo { name: "toZone", data_type: DataType::String, description: "Destination zone" });
    map.insert(24, FieldInfo { name: "tag", data_type: DataType::String, description: "Flow tag" });
    map.insert(25, FieldInfo { name: "overlay", data_type: DataType::String, description: "Overlay identifier" });
    map.insert(26, FieldInfo { name: "direction", data_type: DataType::String, description: "Flow direction" });
    map.insert(27, FieldInfo { name: "applicationCategory", data_type: DataType::String, description: "Application category" });
    
    // Legacy fields
    map.insert(10001, FieldInfo { name: "overlayTunnelID", data_type: DataType::UInt32, description: "Overlay tunnel ID" });
    map.insert(10002, FieldInfo { name: "policyMatchID", data_type: DataType::UInt32, description: "Policy match ID" });
    map.insert(10003, FieldInfo { name: "applianceName", data_type: DataType::String, description: "Appliance name" });
    map.insert(10004, FieldInfo { name: "WANInterfaceID", data_type: DataType::UInt16, description: "WAN interface ID" });
    map.insert(10005, FieldInfo { name: "QOSQueueID", data_type: DataType::UInt8, description: "QoS queue ID" });
    map.insert(10006, FieldInfo { name: "linkQualityMetrics", data_type: DataType::String, description: "Link quality metrics" });
    
    map
}

/// Initialize Cisco enterprise field definitions.
fn init_cisco_fields() -> HashMap<u16, FieldInfo> {
    let mut map = HashMap::new();
    
    // Common Cisco fields
    map.insert(1001, FieldInfo { name: "cisco_application_id", data_type: DataType::UInt32,description: "Cisco application identifier" });
    map.insert(1002, FieldInfo { name: "cisco_application_name", data_type: DataType::String, description: "Cisco application name" });
    map.insert(1003, FieldInfo { name: "cisco_application_category", data_type: DataType::String, description: "Cisco application category" });
    map.insert(1004, FieldInfo { name: "cisco_application_subcategory", data_type: DataType::String, description: "Cisco application subcategory" });
    map.insert(1005, FieldInfo { name: "cisco_application_group", data_type: DataType::String, description: "Cisco application group" });
    map.insert(1006, FieldInfo { name: "cisco_connection_id", data_type: DataType::UInt32, description: "Cisco connection identifier" });
    map.insert(1007, FieldInfo { name: "cisco_service_instance_id", data_type: DataType::UInt32, description: "Cisco service instance ID" });
    map.insert(1008, FieldInfo { name: "cisco_threat_type", data_type: DataType::UInt16, description: "Cisco threat type" });
    map.insert(1009, FieldInfo { name: "cisco_threat_subtype", data_type: DataType::UInt16, description: "Cisco threat subtype" });
    map.insert(1010, FieldInfo { name: "cisco_ssl_server_name", data_type: DataType::String, description: "SSL server name indication" });
    map.insert(1011, FieldInfo { name: "cisco_ssl_actual_encryption", data_type: DataType::UInt16, description: "SSL actual encryption" });
    map.insert(1012, FieldInfo { name: "cisco_ssl_server_cert_status", data_type: DataType::UInt8, description: "SSL server certificate status" });
    map.insert(1013, FieldInfo { name: "cisco_url_category", data_type: DataType::String, description: "URL category" });
    map.insert(1014, FieldInfo { name: "cisco_url_reputation", data_type: DataType::UInt8, description: "URL reputation score" });
    map.insert(1015, FieldInfo { name: "cisco_malware_name", data_type: DataType::String, description: "Malware name" });
    
    map
 }
 
 /// Initialize Juniper enterprise field definitions.
 fn init_juniper_fields() -> HashMap<u16, FieldInfo> {
    let mut map = HashMap::new();
    
    // Common Juniper fields
    map.insert(1001, FieldInfo { name: "juniper_src_vrf_name", data_type: DataType::String, description: "Source VRF name" });
    map.insert(1002, FieldInfo { name: "juniper_dest_vrf_name", data_type: DataType::String, description: "Destination VRF name" });
    map.insert(1003, FieldInfo { name: "juniper_logical_system_name", data_type: DataType::String, description: "Logical system name" });
    map.insert(1004, FieldInfo { name: "juniper_tenant_id", data_type: DataType::UInt32, description: "Tenant identifier" });
    map.insert(1005, FieldInfo { name: "juniper_virtual_router_name", data_type: DataType::String, description: "Virtual router name" });
    map.insert(1006, FieldInfo { name: "juniper_firewall_rule_name", data_type: DataType::String, description: "Firewall rule name" });
    map.insert(1007, FieldInfo { name: "juniper_nat_rule_name", data_type: DataType::String, description: "NAT rule name" });
    map.insert(1008, FieldInfo { name: "juniper_service_set_name", data_type: DataType::String, description: "Service set name" });
    map.insert(1009, FieldInfo { name: "juniper_interface_description", data_type: DataType::String, description: "Interface description" });
    map.insert(1010, FieldInfo { name: "juniper_routing_instance", data_type: DataType::String, description: "Routing instance name" });
    
    map
 }
 
 /// Get protocol name from protocol number.
 fn get_protocol_name(protocol: u8) -> &'static str {
    match protocol {
        0 => "HOPOPT",
        1 => "ICMP",
        2 => "IGMP", 
        3 => "GGP",
        4 => "IPv4",
        5 => "ST",
        6 => "TCP",
        7 => "CBT",
        8 => "EGP",
        9 => "IGP",
        10 => "BBN-RCC-MON",
        11 => "NVP-II",
        12 => "PUP",
        13 => "ARGUS",
        14 => "EMCON",
        15 => "XNET",
        16 => "CHAOS",
        17 => "UDP",
        18 => "MUX",
        19 => "DCN-MEAS",
        20 => "HMP",
        21 => "PRM",
        22 => "XNS-IDP",
        23 => "TRUNK-1",
        24 => "TRUNK-2",
        25 => "LEAF-1",
        26 => "LEAF-2",
        27 => "RDP",
        28 => "IRTP",
        29 => "ISO-TP4",
        30 => "NETBLT",
        31 => "MFE-NSP",
        32 => "MERIT-INP",
        33 => "DCCP",
        34 => "3PC",
        35 => "IDPR",
        36 => "XTP",
        37 => "DDP",
        38 => "IDPR-CMTP",
        39 => "TP++",
        40 => "IL",
        41 => "IPv6",
        42 => "SDRP",
        43 => "IPv6-Route",
        44 => "IPv6-Frag",
        45 => "IDRP",
        46 => "RSVP",
        47 => "GRE",
        48 => "DSR",
        49 => "BNA",
        50 => "ESP",
        51 => "AH",
        52 => "I-NLSP",
        53 => "SWIPE",
        54 => "NARP",
        55 => "MOBILE",
        56 => "TLSP",
        57 => "SKIP",
        58 => "IPv6-ICMP",
        59 => "IPv6-NoNxt",
        60 => "IPv6-Opts",
        61 => "Any-Host-Internal",
        62 => "CFTP",
        63 => "Any-Local-Network",
        64 => "SAT-EXPAK",
        65 => "KRYPTOLAN",
        66 => "RVD",
        67 => "IPPC",
        68 => "Any-Distributed-File-System",
        69 => "SAT-MON",
        70 => "VISA",
        71 => "IPCV",
        72 => "CPNX",
        73 => "CPHB",
        74 => "WSN",
        75 => "PVP",
        76 => "BR-SAT-MON",
        77 => "SUN-ND",
        78 => "WB-MON",
        79 => "WB-EXPAK",
        80 => "ISO-IP",
        81 => "VMTP",
        82 => "SECURE-VMTP",
        83 => "VINES",
        84 => "TTP",
        85 => "NSFNET-IGP",
        86 => "DGP",
        87 => "TCF",
        88 => "EIGRP",
        89 => "OSPFIGP",
        90 => "Sprite-RPC",
        91 => "LARP",
        92 => "MTP",
        93 => "AX.25",
        94 => "IPIP",
        95 => "MICP",
        96 => "SCC-SP",
        97 => "ETHERIP",
        98 => "ENCAP",
        99 => "Any-Private-Encryption",
        100 => "GMTP",
        101 => "IFMP",
        102 => "PNNI",
        103 => "PIM",
        104 => "ARIS",
        105 => "SCPS",
        106 => "QNX",
        107 => "A/N",
        108 => "IPComp",
        109 => "SNP",
        110 => "Compaq-Peer",
        111 => "IPX-in-IP",
        112 => "VRRP",
        113 => "PGM",
        114 => "Any-0-Hop",
        115 => "L2TP",
        116 => "DDX",
        117 => "IATP",
        118 => "STP",
        119 => "SRP",
        120 => "UTI",
        121 => "SMP",
        122 => "SM",
        123 => "PTP",
        124 => "ISIS-over-IPv4",
        125 => "FIRE",
        126 => "CRTP",
        127 => "CRUDP",
        128 => "SSCOPMCE",
        129 => "IPLT",
        130 => "SPS",
        131 => "PIPE",
        132 => "SCTP",
        133 => "FC",
        134 => "RSVP-E2E-IGNORE",
        135 => "Mobility-Header",
        136 => "UDPLite",
        137 => "MPLS-in-IP",
        138 => "MANET",
        139 => "HIP",
        140 => "Shim6",
        141 => "WESP",
        142 => "ROHC",
        143 => "Ethernet",
        253 => "Experimentation",
        254 => "Experimentation",
        255 => "Reserved",
        _ => "Unknown",
    }
 }
 
 #[cfg(test)]
 mod tests {
    use super::*;
    use crate::sources::netflow::config::NetflowConfig;
    use vector_lib::event::LogEvent;
 
    #[test]
    fn test_data_type_parsing() {
        // Test UInt32
        let data = vec![0x00, 0x00, 0x01, 0x00]; // 256
        let result = DataType::UInt32.parse(&data, 1024).unwrap();
        if let Value::Integer(val) = result {
            assert_eq!(val, 256);
        } else {
            panic!("Expected integer value");
        }
 
        // Test IPv4 address
        let data = vec![192, 168, 1, 1];
        let result = DataType::Ipv4Address.parse(&data, 1024).unwrap();
        if let Value::Bytes(bytes) = result {
            assert_eq!(String::from_utf8(bytes.to_vec()).unwrap(), "192.168.1.1");
        } else {
            panic!("Expected bytes value");
        }
 
        // Test MAC address
        let data = vec![0x00, 0x1B, 0x21, 0x3C, 0x4D, 0x5E];
        let result = DataType::MacAddress.parse(&data, 1024).unwrap();
        if let Value::Bytes(bytes) = result {
            assert_eq!(String::from_utf8(bytes.to_vec()).unwrap(), "00:1b:21:3c:4d:5e");
        } else {
            panic!("Expected bytes value");
        }
 
        // Test string
        let data = b"Hello World\0";
        let result = DataType::String.parse(data, 1024).unwrap();
        if let Value::Bytes(bytes) = result {
            assert_eq!(String::from_utf8(bytes.to_vec()).unwrap(), "Hello World");
        } else {
            panic!("Expected bytes value");
        }
 
        // Test binary (invalid UTF-8)
        let data = vec![0xFF, 0xFE, 0xFD];
        let result = DataType::Binary.parse(&data, 1024).unwrap();
        if let Value::Bytes(bytes) = result {
            let decoded = base64::engine::general_purpose::STANDARD.decode(&bytes).unwrap();
            assert_eq!(decoded, data);
        } else {
            panic!("Expected bytes value");
        }
    }
 
    #[test]
    fn test_field_parser_creation() {
        let config = NetflowConfig::default();
        let parser = FieldParser::new(&config);
        assert_eq!(parser.max_field_length, 1024);
        assert!(parser.resolve_protocols);
    }
 
    #[test]
    fn test_standard_field_lookup() {
        let config = NetflowConfig::default();
        let parser = FieldParser::new(&config);
        
        let field = TemplateField {
            field_type: 8, // sourceIPv4Address
            field_length: 4,
            enterprise_number: None,
        };
        
        let field_info = parser.get_field_info(&field);
        assert_eq!(field_info.name, "sourceIPv4Address");
        assert!(matches!(field_info.data_type, DataType::Ipv4Address));
    }
 
    #[test]
    fn test_hpe_aruba_field_lookup() {
        let config = NetflowConfig::default();
        let parser = FieldParser::new(&config);
        
        let field = TemplateField {
            field_type: 1, // clientIPv4Address
            field_length: 4,
            enterprise_number: Some(23867),
        };
        
        let field_info = parser.get_field_info(&field);
        assert_eq!(field_info.name, "clientIPv4Address");
        assert!(matches!(field_info.data_type, DataType::Ipv4Address));
    }
 
    #[test]
    fn test_custom_enterprise_field() {
        let mut config = NetflowConfig::default();
        config.enterprise_fields.insert(
            "9:1001".to_string(),
            "custom_cisco_field".to_string(),
        );
        
        let parser = FieldParser::new(&config);
        
        let field = TemplateField {
            field_type: 1001,
            field_length: 4,
            enterprise_number: Some(9),
        };
        
        let field_info = parser.get_field_info(&field);
        assert_eq!(field_info.name, "custom_cisco_field");
        assert!(matches!(field_info.data_type, DataType::UInt32));
    }
 
    #[test]
    fn test_unknown_field_handling() {
        let config = NetflowConfig::default();
        let parser = FieldParser::new(&config);
        
        // Unknown standard field
        let field = TemplateField {
            field_type: 9999,
            field_length: 4,
            enterprise_number: None,
        };
        
        let field_info = parser.get_field_info(&field);
        assert_eq!(field_info.name, "unknown_field_9999");
        assert!(matches!(field_info.data_type, DataType::Binary));
        
        // Unknown enterprise field
        let field = TemplateField {
            field_type: 1001,
            field_length: 4,
            enterprise_number: Some(99999),
        };
        
        let field_info = parser.get_field_info(&field);
        assert_eq!(field_info.name, "enterprise_99999_1001");
        assert!(matches!(field_info.data_type, DataType::Binary));
    }
 
    #[test]
    fn test_field_parsing_with_protocol_resolution() {
        let config = NetflowConfig::default();
        let parser = FieldParser::new(&config);
        
        let field = TemplateField {
            field_type: 4, // protocolIdentifier
            field_length: 1,
            enterprise_number: None,
        };
        
        let mut log_event = LogEvent::default();
        let data = vec![6]; // TCP
        
        parser.parse_field(&field, &data, &mut log_event);
        
        assert_eq!(log_event.get("protocolIdentifier").unwrap().as_integer().unwrap(), 6);
        assert_eq!(log_event.get("protocolName").unwrap().as_str().unwrap(), "TCP");
    }
 
    #[test]
    fn test_field_truncation() {
        let config = NetflowConfig {
            max_field_length: 10,
            ..Default::default()
        };
        let parser = FieldParser::new(&config);
        
        let field = TemplateField {
            field_type: 999, // Unknown field, will be parsed as binary
            field_length: 20,
            enterprise_number: None,
        };
        
        let mut log_event = LogEvent::default();
        let data = vec![0x41; 20]; // 20 'A' characters
        
        parser.parse_field(&field, &data, &mut log_event);
        
        let value = log_event.get("unknown_field_999").unwrap().as_str().unwrap();
        assert!(value.len() <= 10);
        assert!(value.ends_with("..."));
    }
 
    #[test]
    fn test_protocol_name_resolution() {
        assert_eq!(get_protocol_name(1), "ICMP");
        assert_eq!(get_protocol_name(6), "TCP");
        assert_eq!(get_protocol_name(17), "UDP");
        assert_eq!(get_protocol_name(47), "GRE");
        assert_eq!(get_protocol_name(50), "ESP");
        assert_eq!(get_protocol_name(255), "Reserved");
        assert_eq!(get_protocol_name(200), "Unknown");
    }
 
    #[test]
    fn test_insufficient_data_handling() {
        let data = vec![0x01]; // Only 1 byte
        
        // Should fail for UInt32 (needs 4 bytes)
        let result = DataType::UInt32.parse(&data, 1024);
        assert!(result.is_err());
        
        // Should succeed for UInt8 (needs 1 byte)
        let result = DataType::UInt8.parse(&data, 1024);
        assert!(result.is_ok());
    }
 
    #[test]
    fn test_data_type_from_field_type() {
        assert!(matches!(DataType::from(FieldType::Uint32), DataType::UInt32));
        assert!(matches!(DataType::from(FieldType::Ipv4Address), DataType::Ipv4Address));
        assert!(matches!(DataType::from(FieldType::String), DataType::String));
        assert!(matches!(DataType::from(FieldType::Binary), DataType::Binary));
    }
 }