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
                    // Handle large unsigned values that would overflow i32
                    if value > i32::MAX as u32 {
                        // For values > i32::MAX, store as string to avoid PostgreSQL integer overflow
                        debug!(
                            "UInt32 value {} exceeds i32::MAX, storing as string to avoid PostgreSQL overflow",
                            value
                        );
                        Ok(Value::Bytes(value.to_string().into()))
                    } else {
                        Ok(Value::Integer(value as i64))
                    }
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
            max_field_length: config.max_packet_size,
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
    
    map.insert(1, FieldInfo { name: "octetDeltaCount", data_type: DataType::UInt64, description: "The number of octets since the" });
    map.insert(2, FieldInfo { name: "packetDeltaCount", data_type: DataType::UInt64, description: "The number of incoming packets since" });
    map.insert(3, FieldInfo { name: "deltaFlowCount", data_type: DataType::UInt64, description: "The conservative count of Original Flows" });
    map.insert(4, FieldInfo { name: "protocolIdentifier", data_type: DataType::UInt8, description: "The value of the protocol number" });
    map.insert(5, FieldInfo { name: "ipClassOfService", data_type: DataType::UInt8, description: "For IPv4 packets, this is the" });
    map.insert(6, FieldInfo { name: "tcpControlBits", data_type: DataType::UInt16, description: "TCP control bits observed for the" });
    map.insert(7, FieldInfo { name: "sourceTransportPort", data_type: DataType::UInt16, description: "The source port identifier in the" });
    map.insert(8, FieldInfo { name: "sourceIPv4Address", data_type: DataType::Ipv4Address, description: "The IPv4 source address in the" });
    map.insert(9, FieldInfo { name: "sourceIPv4PrefixLength", data_type: DataType::UInt8, description: "The number of contiguous bits that" });
    map.insert(10, FieldInfo { name: "ingressInterface", data_type: DataType::UInt32, description: "The index of the IP interface" });
    map.insert(11, FieldInfo { name: "destinationTransportPort", data_type: DataType::UInt16, description: "The destination port identifier in the" });
    map.insert(12, FieldInfo { name: "destinationIPv4Address", data_type: DataType::Ipv4Address, description: "The IPv4 destination address in the" });
    map.insert(13, FieldInfo { name: "destinationIPv4PrefixLength", data_type: DataType::UInt8, description: "The number of contiguous bits that" });
    map.insert(14, FieldInfo { name: "egressInterface", data_type: DataType::UInt32, description: "The index of the IP interface" });
    map.insert(15, FieldInfo { name: "ipNextHopIPv4Address", data_type: DataType::Ipv4Address, description: "The IPv4 address of the next" });
    map.insert(16, FieldInfo { name: "bgpSourceAsNumber", data_type: DataType::UInt32, description: "The autonomous system (AS) number of" });
    map.insert(17, FieldInfo { name: "bgpDestinationAsNumber", data_type: DataType::UInt32, description: "The autonomous system (AS) number of" });
    map.insert(18, FieldInfo { name: "bgpNextHopIPv4Address", data_type: DataType::Ipv4Address, description: "The IPv4 address of the next" });
    map.insert(19, FieldInfo { name: "postMCastPacketDeltaCount", data_type: DataType::UInt64, description: "The number of outgoing multicast packets" });
    map.insert(20, FieldInfo { name: "postMCastOctetDeltaCount", data_type: DataType::UInt64, description: "The number of octets since the" });
    map.insert(21, FieldInfo { name: "flowEndSysUpTime", data_type: DataType::UInt32, description: "The relative timestamp of the last" });
    map.insert(22, FieldInfo { name: "flowStartSysUpTime", data_type: DataType::UInt32, description: "The relative timestamp of the first" });
    map.insert(23, FieldInfo { name: "postOctetDeltaCount", data_type: DataType::UInt64, description: "The definition of this Information Element" });
    map.insert(24, FieldInfo { name: "postPacketDeltaCount", data_type: DataType::UInt64, description: "The definition of this Information Element" });
    map.insert(25, FieldInfo { name: "minimumIpTotalLength", data_type: DataType::UInt64, description: "Length of the smallest packet observed" });
    map.insert(26, FieldInfo { name: "maximumIpTotalLength", data_type: DataType::UInt64, description: "Length of the largest packet observed" });
    map.insert(27, FieldInfo { name: "sourceIPv6Address", data_type: DataType::Ipv6Address, description: "The IPv6 source address in the" });
    map.insert(28, FieldInfo { name: "destinationIPv6Address", data_type: DataType::Ipv6Address, description: "The IPv6 destination address in the" });
    map.insert(29, FieldInfo { name: "sourceIPv6PrefixLength", data_type: DataType::UInt8, description: "The number of contiguous bits that" });
    map.insert(30, FieldInfo { name: "destinationIPv6PrefixLength", data_type: DataType::UInt8, description: "The number of contiguous bits that" });
    map.insert(31, FieldInfo { name: "flowLabelIPv6", data_type: DataType::UInt32, description: "The value of the IPv6 Flow" });
    map.insert(32, FieldInfo { name: "icmpTypeCodeIPv4", data_type: DataType::UInt16, description: "Type and Code of the IPv4" });
    map.insert(33, FieldInfo { name: "igmpType", data_type: DataType::UInt8, description: "The type field of the IGMP" });
    map.insert(34, FieldInfo { name: "samplingInterval", data_type: DataType::UInt32, description: "Deprecated in favor of 305 samplingPacketInterval." });
    map.insert(35, FieldInfo { name: "samplingAlgorithm", data_type: DataType::UInt8, description: "Deprecated in favor of 304 selectorAlgorithm." });
    map.insert(36, FieldInfo { name: "flowActiveTimeout", data_type: DataType::UInt16, description: "The number of seconds after which" });
    map.insert(37, FieldInfo { name: "flowIdleTimeout", data_type: DataType::UInt16, description: "A Flow is considered to be" });
    map.insert(38, FieldInfo { name: "engineType", data_type: DataType::UInt8, description: "Type of flow switching engine in" });
    map.insert(39, FieldInfo { name: "engineId", data_type: DataType::UInt8, description: "Versatile Interface Processor (VIP) or line" });
    map.insert(40, FieldInfo { name: "exportedOctetTotalCount", data_type: DataType::UInt64, description: "The total number of octets that" });
    map.insert(41, FieldInfo { name: "exportedMessageTotalCount", data_type: DataType::UInt64, description: "The total number of IPFIX Messages" });
    map.insert(42, FieldInfo { name: "exportedFlowRecordTotalCount", data_type: DataType::UInt64, description: "The total number of Flow Records" });
    map.insert(43, FieldInfo { name: "ipv4RouterSc", data_type: DataType::Ipv4Address, description: "This is a platform-specific field for" });
    map.insert(44, FieldInfo { name: "sourceIPv4Prefix", data_type: DataType::Ipv4Address, description: "IPv4 source address prefix." });
    map.insert(45, FieldInfo { name: "destinationIPv4Prefix", data_type: DataType::Ipv4Address, description: "IPv4 destination address prefix." });
    map.insert(46, FieldInfo { name: "mplsTopLabelType", data_type: DataType::UInt8, description: "This field identifies the control protocol" });
    map.insert(47, FieldInfo { name: "mplsTopLabelIPv4Address", data_type: DataType::Ipv4Address, description: "The IPv4 address of the system" });
    map.insert(48, FieldInfo { name: "samplerId", data_type: DataType::UInt8, description: "Deprecated in favor of 302 selectorId." });
    map.insert(49, FieldInfo { name: "samplerMode", data_type: DataType::UInt8, description: "Deprecated in favor of 304 selectorAlgorithm." });
    map.insert(50, FieldInfo { name: "samplerRandomInterval", data_type: DataType::UInt32, description: "Deprecated in favor of 305 samplingPacketInterval." });
    map.insert(51, FieldInfo { name: "classId", data_type: DataType::UInt8, description: "Deprecated in favor of 302 selectorId." });
    map.insert(52, FieldInfo { name: "minimumTTL", data_type: DataType::UInt8, description: "Minimum TTL value observed for any" });
    map.insert(53, FieldInfo { name: "maximumTTL", data_type: DataType::UInt8, description: "Maximum TTL value observed for any" });
    map.insert(54, FieldInfo { name: "fragmentIdentification", data_type: DataType::UInt32, description: "The value of the Identification field" });
    map.insert(55, FieldInfo { name: "postIpClassOfService", data_type: DataType::UInt8, description: "The definition of this Information Element" });
    map.insert(56, FieldInfo { name: "sourceMacAddress", data_type: DataType::MacAddress, description: "The IEEE 802 source MAC address" });
    map.insert(57, FieldInfo { name: "postDestinationMacAddress", data_type: DataType::MacAddress, description: "The definition of this Information Element" });
    map.insert(58, FieldInfo { name: "vlanId", data_type: DataType::UInt16, description: "Virtual LAN identifier associated with ingress" });
    map.insert(59, FieldInfo { name: "postVlanId", data_type: DataType::UInt16, description: "Virtual LAN identifier associated with egress" });
    map.insert(60, FieldInfo { name: "ipVersion", data_type: DataType::UInt8, description: "The IP version field in the" });
    map.insert(61, FieldInfo { name: "flowDirection", data_type: DataType::UInt8, description: "The direction of the Flow observed" });
    map.insert(62, FieldInfo { name: "ipNextHopIPv6Address", data_type: DataType::Ipv6Address, description: "The IPv6 address of the next" });
    map.insert(63, FieldInfo { name: "bgpNextHopIPv6Address", data_type: DataType::Ipv6Address, description: "The IPv6 address of the next" });
    map.insert(64, FieldInfo { name: "ipv6ExtensionHeaders", data_type: DataType::UInt32, description: "Deprecated in favor of the ipv6ExtensionHeadersFull" });
    map.insert(70, FieldInfo { name: "mplsTopLabelStackSection", data_type: DataType::Binary, description: "The Label, Exp, and S fields" });
    map.insert(71, FieldInfo { name: "mplsLabelStackSection2", data_type: DataType::Binary, description: "The Label, Exp, and S fields" });
    map.insert(72, FieldInfo { name: "mplsLabelStackSection3", data_type: DataType::Binary, description: "The Label, Exp, and S fields" });
    map.insert(73, FieldInfo { name: "mplsLabelStackSection4", data_type: DataType::Binary, description: "The Label, Exp, and S fields" });
    map.insert(74, FieldInfo { name: "mplsLabelStackSection5", data_type: DataType::Binary, description: "The Label, Exp, and S fields" });
    map.insert(75, FieldInfo { name: "mplsLabelStackSection6", data_type: DataType::Binary, description: "The Label, Exp, and S fields" });
    map.insert(76, FieldInfo { name: "mplsLabelStackSection7", data_type: DataType::Binary, description: "The Label, Exp, and S fields" });
    map.insert(77, FieldInfo { name: "mplsLabelStackSection8", data_type: DataType::Binary, description: "The Label, Exp, and S fields" });
    map.insert(78, FieldInfo { name: "mplsLabelStackSection9", data_type: DataType::Binary, description: "The Label, Exp, and S fields" });
    map.insert(79, FieldInfo { name: "mplsLabelStackSection10", data_type: DataType::Binary, description: "The Label, Exp, and S fields" });
    map.insert(80, FieldInfo { name: "destinationMacAddress", data_type: DataType::MacAddress, description: "The IEEE 802 destination MAC address" });
    map.insert(81, FieldInfo { name: "postSourceMacAddress", data_type: DataType::MacAddress, description: "The definition of this Information Element" });
    map.insert(82, FieldInfo { name: "interfaceName", data_type: DataType::String, description: "A short name uniquely describing an" });
    map.insert(83, FieldInfo { name: "interfaceDescription", data_type: DataType::String, description: "The description of an interface, eg" });
    map.insert(84, FieldInfo { name: "samplerName", data_type: DataType::String, description: "Deprecated in favor of 335 selectorName." });
    map.insert(85, FieldInfo { name: "octetTotalCount", data_type: DataType::UInt64, description: "The total number of octets in" });
    map.insert(86, FieldInfo { name: "packetTotalCount", data_type: DataType::UInt64, description: "The total number of incoming packets" });
    map.insert(87, FieldInfo { name: "flagsAndSamplerId", data_type: DataType::UInt32, description: "Flow flags and the value of" });
    map.insert(88, FieldInfo { name: "fragmentOffset", data_type: DataType::UInt16, description: "The value of the IP fragment" });
    map.insert(89, FieldInfo { name: "forwardingStatus", data_type: DataType::UInt8, description: "This Information Element describes the forwarding" });
    map.insert(90, FieldInfo { name: "mplsVpnRouteDistinguisher", data_type: DataType::Binary, description: "The value of the VPN route" });
    map.insert(91, FieldInfo { name: "mplsTopLabelPrefixLength", data_type: DataType::UInt8, description: "The prefix length of the subnet" });
    map.insert(92, FieldInfo { name: "srcTrafficIndex", data_type: DataType::UInt32, description: "BGP Policy Accounting Source Traffic Index." });
    map.insert(93, FieldInfo { name: "dstTrafficIndex", data_type: DataType::UInt32, description: "BGP Policy Accounting Destination Traffic Index." });
    map.insert(94, FieldInfo { name: "applicationDescription", data_type: DataType::String, description: "Specifies the description of an application." });
    map.insert(95, FieldInfo { name: "applicationId", data_type: DataType::Binary, description: "Specifies an Application ID per [RFC6759]." });
    map.insert(96, FieldInfo { name: "applicationName", data_type: DataType::String, description: "Specifies the name of an application." });
    map.insert(97, FieldInfo { name: "Assigned for NetFlow v9 compatibility", data_type: DataType::String, description: "" });
    map.insert(98, FieldInfo { name: "postIpDiffServCodePoint", data_type: DataType::UInt8, description: "The definition of this Information Element" });
    map.insert(99, FieldInfo { name: "multicastReplicationFactor", data_type: DataType::UInt32, description: "The amount of multicast replication that's" });
    map.insert(100, FieldInfo { name: "className", data_type: DataType::String, description: "Deprecated in favor of 335 selectorName." });
    map.insert(101, FieldInfo { name: "classificationEngineId", data_type: DataType::UInt8, description: "A unique identifier for the engine" });
    map.insert(102, FieldInfo { name: "layer2packetSectionOffset", data_type: DataType::UInt16, description: "Deprecated in favor of 409 sectionOffset." });
    map.insert(103, FieldInfo { name: "layer2packetSectionSize", data_type: DataType::UInt16, description: "Deprecated in favor of 312 dataLinkFrameSize." });
    map.insert(104, FieldInfo { name: "layer2packetSectionData", data_type: DataType::Binary, description: "Deprecated in favor of 315 dataLinkFrameSection." });
    map.insert(128, FieldInfo { name: "bgpNextAdjacentAsNumber", data_type: DataType::UInt32, description: "The autonomous system (AS) number of" });
    map.insert(129, FieldInfo { name: "bgpPrevAdjacentAsNumber", data_type: DataType::UInt32, description: "The autonomous system (AS) number of" });
    map.insert(130, FieldInfo { name: "exporterIPv4Address", data_type: DataType::Ipv4Address, description: "The IPv4 address used by the" });
    map.insert(131, FieldInfo { name: "exporterIPv6Address", data_type: DataType::Ipv6Address, description: "The IPv6 address used by the" });
    map.insert(132, FieldInfo { name: "droppedOctetDeltaCount", data_type: DataType::UInt64, description: "The number of octets since the" });
    map.insert(133, FieldInfo { name: "droppedPacketDeltaCount", data_type: DataType::UInt64, description: "The number of packets since the" });
    map.insert(134, FieldInfo { name: "droppedOctetTotalCount", data_type: DataType::UInt64, description: "The total number of octets in" });
    map.insert(135, FieldInfo { name: "droppedPacketTotalCount", data_type: DataType::UInt64, description: "The number of packets of this" });
    map.insert(136, FieldInfo { name: "flowEndReason", data_type: DataType::UInt8, description: "The reason for Flow termination. Values" });
    map.insert(137, FieldInfo { name: "commonPropertiesId", data_type: DataType::UInt64, description: "An identifier of a set of" });
    map.insert(138, FieldInfo { name: "observationPointId", data_type: DataType::UInt64, description: "An identifier of an Observation Point" });
    map.insert(139, FieldInfo { name: "icmpTypeCodeIPv6", data_type: DataType::UInt16, description: "Type and Code of the IPv6" });
    map.insert(140, FieldInfo { name: "mplsTopLabelIPv6Address", data_type: DataType::Ipv6Address, description: "The IPv6 address of the system" });
    map.insert(141, FieldInfo { name: "lineCardId", data_type: DataType::UInt32, description: "An identifier of a line card" });
    map.insert(142, FieldInfo { name: "portId", data_type: DataType::UInt32, description: "An identifier of a line port" });
    map.insert(143, FieldInfo { name: "meteringProcessId", data_type: DataType::UInt32, description: "An identifier of a Metering Process" });
    map.insert(144, FieldInfo { name: "exportingProcessId", data_type: DataType::UInt32, description: "An identifier of an Exporting Process" });
    map.insert(145, FieldInfo { name: "templateId", data_type: DataType::UInt16, description: "An identifier of a Template that" });
    map.insert(146, FieldInfo { name: "wlanChannelId", data_type: DataType::UInt8, description: "The identifier of the 802.11 (Wi-Fi)" });
    map.insert(147, FieldInfo { name: "wlanSSID", data_type: DataType::String, description: "The Service Set IDentifier (SSID) identifying" });
    map.insert(148, FieldInfo { name: "flowId", data_type: DataType::UInt64, description: "An identifier of a Flow that" });
    map.insert(149, FieldInfo { name: "observationDomainId", data_type: DataType::UInt32, description: "An identifier of an Observation Domain" });
    map.insert(150, FieldInfo { name: "flowStartSeconds", data_type: DataType::DateTimeSeconds, description: "The absolute timestamp of the first" });
    map.insert(151, FieldInfo { name: "flowEndSeconds", data_type: DataType::DateTimeSeconds, description: "The absolute timestamp of the last" });
    map.insert(152, FieldInfo { name: "flowStartMilliseconds", data_type: DataType::DateTimeMilliseconds, description: "The absolute timestamp of the first" });
    map.insert(153, FieldInfo { name: "flowEndMilliseconds", data_type: DataType::DateTimeMilliseconds, description: "The absolute timestamp of the last" });
    map.insert(154, FieldInfo { name: "flowStartMicroseconds", data_type: DataType::DateTimeMicroseconds, description: "The absolute timestamp of the first" });
    map.insert(155, FieldInfo { name: "flowEndMicroseconds", data_type: DataType::DateTimeMicroseconds, description: "The absolute timestamp of the last" });
    map.insert(156, FieldInfo { name: "flowStartNanoseconds", data_type: DataType::DateTimeNanoseconds, description: "The absolute timestamp of the first" });
    map.insert(157, FieldInfo { name: "flowEndNanoseconds", data_type: DataType::DateTimeNanoseconds, description: "The absolute timestamp of the last" });
    map.insert(158, FieldInfo { name: "flowStartDeltaMicroseconds", data_type: DataType::UInt32, description: "This is a relative timestamp only" });
    map.insert(159, FieldInfo { name: "flowEndDeltaMicroseconds", data_type: DataType::UInt32, description: "This is a relative timestamp only" });
    map.insert(160, FieldInfo { name: "systemInitTimeMilliseconds", data_type: DataType::DateTimeMilliseconds, description: "The absolute timestamp of the last" });
    map.insert(161, FieldInfo { name: "flowDurationMilliseconds", data_type: DataType::UInt32, description: "The difference in time between the" });
    map.insert(162, FieldInfo { name: "flowDurationMicroseconds", data_type: DataType::UInt32, description: "The difference in time between the" });
    map.insert(163, FieldInfo { name: "observedFlowTotalCount", data_type: DataType::UInt64, description: "The total number of Flows observed" });
    map.insert(164, FieldInfo { name: "ignoredPacketTotalCount", data_type: DataType::UInt64, description: "The total number of observed IP" });
    map.insert(165, FieldInfo { name: "ignoredOctetTotalCount", data_type: DataType::UInt64, description: "The total number of octets in" });
    map.insert(166, FieldInfo { name: "notSentFlowTotalCount", data_type: DataType::UInt64, description: "The total number of Flow Records" });
    map.insert(167, FieldInfo { name: "notSentPacketTotalCount", data_type: DataType::UInt64, description: "The total number of packets in" });
    map.insert(168, FieldInfo { name: "notSentOctetTotalCount", data_type: DataType::UInt64, description: "The total number of octets in" });
    map.insert(169, FieldInfo { name: "destinationIPv6Prefix", data_type: DataType::Ipv6Address, description: "IPv6 destination address prefix." });
    map.insert(170, FieldInfo { name: "sourceIPv6Prefix", data_type: DataType::Ipv6Address, description: "IPv6 source address prefix." });
    map.insert(171, FieldInfo { name: "postOctetTotalCount", data_type: DataType::UInt64, description: "The definition of this Information Element" });
    map.insert(172, FieldInfo { name: "postPacketTotalCount", data_type: DataType::UInt64, description: "The definition of this Information Element" });
    map.insert(173, FieldInfo { name: "flowKeyIndicator", data_type: DataType::UInt64, description: "This set of bit fields is" });
    map.insert(174, FieldInfo { name: "postMCastPacketTotalCount", data_type: DataType::UInt64, description: "The total number of outgoing multicast" });
    map.insert(175, FieldInfo { name: "postMCastOctetTotalCount", data_type: DataType::UInt64, description: "The total number of octets in" });
    map.insert(176, FieldInfo { name: "icmpTypeIPv4", data_type: DataType::UInt8, description: "Type of the IPv4 ICMP message." });
    map.insert(177, FieldInfo { name: "icmpCodeIPv4", data_type: DataType::UInt8, description: "Code of the IPv4 ICMP message." });
    map.insert(178, FieldInfo { name: "icmpTypeIPv6", data_type: DataType::UInt8, description: "Type of the IPv6 ICMP message." });
    map.insert(179, FieldInfo { name: "icmpCodeIPv6", data_type: DataType::UInt8, description: "Code of the IPv6 ICMP message." });
    map.insert(180, FieldInfo { name: "udpSourcePort", data_type: DataType::UInt16, description: "The source port identifier in the" });
    map.insert(181, FieldInfo { name: "udpDestinationPort", data_type: DataType::UInt16, description: "The destination port identifier in the" });
    map.insert(182, FieldInfo { name: "tcpSourcePort", data_type: DataType::UInt16, description: "The source port identifier in the" });
    map.insert(183, FieldInfo { name: "tcpDestinationPort", data_type: DataType::UInt16, description: "The destination port identifier in the" });
    map.insert(184, FieldInfo { name: "tcpSequenceNumber", data_type: DataType::UInt32, description: "The sequence number in the TCP" });
    map.insert(185, FieldInfo { name: "tcpAcknowledgementNumber", data_type: DataType::UInt32, description: "The acknowledgement number in the TCP" });
    map.insert(186, FieldInfo { name: "tcpWindowSize", data_type: DataType::UInt16, description: "The window field in the TCP" });
    map.insert(187, FieldInfo { name: "tcpUrgentPointer", data_type: DataType::UInt16, description: "The urgent pointer in the TCP" });
    map.insert(188, FieldInfo { name: "tcpHeaderLength", data_type: DataType::UInt8, description: "The length of the TCP header." });
    map.insert(189, FieldInfo { name: "ipHeaderLength", data_type: DataType::UInt8, description: "The length of the IP header." });
    map.insert(190, FieldInfo { name: "totalLengthIPv4", data_type: DataType::UInt16, description: "The total length of the IPv4" });
    map.insert(191, FieldInfo { name: "payloadLengthIPv6", data_type: DataType::UInt16, description: "This Information Element reports the value" });
    map.insert(192, FieldInfo { name: "ipTTL", data_type: DataType::UInt8, description: "For IPv4, the value of the" });
    map.insert(193, FieldInfo { name: "nextHeaderIPv6", data_type: DataType::UInt8, description: "The value of the Next Header" });
    map.insert(194, FieldInfo { name: "mplsPayloadLength", data_type: DataType::UInt32, description: "The size of the MPLS packet" });
    map.insert(195, FieldInfo { name: "ipDiffServCodePoint", data_type: DataType::UInt8, description: "The value of a Differentiated Services" });
    map.insert(196, FieldInfo { name: "ipPrecedence", data_type: DataType::UInt8, description: "The value of the IP Precedence." });
    map.insert(197, FieldInfo { name: "fragmentFlags", data_type: DataType::UInt8, description: "Fragmentation properties indicated by flags in" });
    map.insert(198, FieldInfo { name: "octetDeltaSumOfSquares", data_type: DataType::UInt64, description: "The sum of the squared numbers" });
    map.insert(199, FieldInfo { name: "octetTotalSumOfSquares", data_type: DataType::UInt64, description: "The total sum of the squared" });
    map.insert(200, FieldInfo { name: "mplsTopLabelTTL", data_type: DataType::UInt8, description: "The TTL field from the top" });
    map.insert(201, FieldInfo { name: "mplsLabelStackLength", data_type: DataType::UInt32, description: "The length of the MPLS label" });
    map.insert(202, FieldInfo { name: "mplsLabelStackDepth", data_type: DataType::UInt32, description: "The number of labels in the" });
    map.insert(203, FieldInfo { name: "mplsTopLabelExp", data_type: DataType::UInt8, description: "The Exp field from the top" });
    map.insert(204, FieldInfo { name: "ipPayloadLength", data_type: DataType::UInt32, description: "The effective length of the IP" });
    map.insert(205, FieldInfo { name: "udpMessageLength", data_type: DataType::UInt16, description: "The value of the Length field" });
    map.insert(206, FieldInfo { name: "isMulticast", data_type: DataType::UInt8, description: "If the IP destination address is" });
    map.insert(207, FieldInfo { name: "ipv4IHL", data_type: DataType::UInt8, description: "The value of the Internet Header" });
    map.insert(208, FieldInfo { name: "ipv4Options", data_type: DataType::UInt32, description: "IPv4 options in packets of this" });
    map.insert(209, FieldInfo { name: "tcpOptions", data_type: DataType::UInt64, description: "Deprecated in favor of the tcpOptionsFull" });
    map.insert(210, FieldInfo { name: "paddingOctets", data_type: DataType::Binary, description: "The value of this Information Element" });
    map.insert(211, FieldInfo { name: "collectorIPv4Address", data_type: DataType::Ipv4Address, description: "An IPv4 address to which the" });
    map.insert(212, FieldInfo { name: "collectorIPv6Address", data_type: DataType::Ipv6Address, description: "An IPv6 address to which the" });
    map.insert(213, FieldInfo { name: "exportInterface", data_type: DataType::UInt32, description: "The index of the interface from" });
    map.insert(214, FieldInfo { name: "exportProtocolVersion", data_type: DataType::UInt8, description: "The protocol version used by the" });
    map.insert(215, FieldInfo { name: "exportTransportProtocol", data_type: DataType::UInt8, description: "The value of the protocol number" });
    map.insert(216, FieldInfo { name: "collectorTransportPort", data_type: DataType::UInt16, description: "The destination port identifier to which" });
    map.insert(217, FieldInfo { name: "exporterTransportPort", data_type: DataType::UInt16, description: "The source port identifier from which" });
    map.insert(218, FieldInfo { name: "tcpSynTotalCount", data_type: DataType::UInt64, description: "The total number of packets of" });
    map.insert(219, FieldInfo { name: "tcpFinTotalCount", data_type: DataType::UInt64, description: "The total number of packets of" });
    map.insert(220, FieldInfo { name: "tcpRstTotalCount", data_type: DataType::UInt64, description: "The total number of packets of" });
    map.insert(221, FieldInfo { name: "tcpPshTotalCount", data_type: DataType::UInt64, description: "The total number of packets of" });
    map.insert(222, FieldInfo { name: "tcpAckTotalCount", data_type: DataType::UInt64, description: "The total number of packets of" });
    map.insert(223, FieldInfo { name: "tcpUrgTotalCount", data_type: DataType::UInt64, description: "The total number of packets of" });
    map.insert(224, FieldInfo { name: "ipTotalLength", data_type: DataType::UInt64, description: "The total length of the IP" });
    map.insert(225, FieldInfo { name: "postNATSourceIPv4Address", data_type: DataType::Ipv4Address, description: "The definition of this Information Element" });
    map.insert(226, FieldInfo { name: "postNATDestinationIPv4Address", data_type: DataType::Ipv4Address, description: "The definition of this Information Element" });
    map.insert(227, FieldInfo { name: "postNAPTSourceTransportPort", data_type: DataType::UInt16, description: "The definition of this Information Element" });
    map.insert(228, FieldInfo { name: "postNAPTDestinationTransportPort", data_type: DataType::UInt16, description: "The definition of this Information Element" });
    map.insert(229, FieldInfo { name: "natOriginatingAddressRealm", data_type: DataType::UInt8, description: "Indicates whether the session was created" });
    map.insert(230, FieldInfo { name: "natEvent", data_type: DataType::UInt8, description: "This Information Element identifies a NAT" });
    map.insert(231, FieldInfo { name: "initiatorOctets", data_type: DataType::UInt64, description: "The total number of layer 4" });
    map.insert(232, FieldInfo { name: "responderOctets", data_type: DataType::UInt64, description: "The total number of layer 4" });
    map.insert(233, FieldInfo { name: "firewallEvent", data_type: DataType::UInt8, description: "Indicates a firewall event. Allowed values" });
    map.insert(234, FieldInfo { name: "ingressVRFID", data_type: DataType::UInt32, description: "An unique identifier of the VRFname" });
    map.insert(235, FieldInfo { name: "egressVRFID", data_type: DataType::UInt32, description: "An unique identifier of the VRFname" });
    map.insert(236, FieldInfo { name: "VRFname", data_type: DataType::String, description: "The name of a VPN Routing" });
    map.insert(237, FieldInfo { name: "postMplsTopLabelExp", data_type: DataType::UInt8, description: "The definition of this Information Element" });
    map.insert(238, FieldInfo { name: "tcpWindowScale", data_type: DataType::UInt16, description: "The scale of the window field" });
    map.insert(239, FieldInfo { name: "biflowDirection", data_type: DataType::UInt8, description: "A description of the direction assignment" });
    map.insert(240, FieldInfo { name: "ethernetHeaderLength", data_type: DataType::UInt8, description: "The difference between the length of" });
    map.insert(241, FieldInfo { name: "ethernetPayloadLength", data_type: DataType::UInt16, description: "The length of the MAC Client" });
    map.insert(242, FieldInfo { name: "ethernetTotalLength", data_type: DataType::UInt16, description: "The total length of the Ethernet" });
    map.insert(243, FieldInfo { name: "dot1qVlanId", data_type: DataType::UInt16, description: "The value of the 12-bit VLAN" });
    map.insert(244, FieldInfo { name: "dot1qPriority", data_type: DataType::UInt8, description: "The value of the 3-bit User" });
    map.insert(245, FieldInfo { name: "dot1qCustomerVlanId", data_type: DataType::UInt16, description: "The value represents the Customer VLAN" });
    map.insert(246, FieldInfo { name: "dot1qCustomerPriority", data_type: DataType::UInt8, description: "The value represents the 3-bit Priority" });
    map.insert(247, FieldInfo { name: "metroEvcId", data_type: DataType::String, description: "The EVC Service Attribute which uniquely" });
    map.insert(248, FieldInfo { name: "metroEvcType", data_type: DataType::UInt8, description: "The 3-bit EVC Service Attribute which" });
    map.insert(249, FieldInfo { name: "pseudoWireId", data_type: DataType::UInt32, description: "A 32-bit non-zero connection identifier, which" });
    map.insert(250, FieldInfo { name: "pseudoWireType", data_type: DataType::UInt16, description: "The value of this information element" });
    map.insert(251, FieldInfo { name: "pseudoWireControlWord", data_type: DataType::UInt32, description: "The 32-bit Preferred Pseudo Wire (PW)" });
    map.insert(252, FieldInfo { name: "ingressPhysicalInterface", data_type: DataType::UInt32, description: "The index of a networking device's" });
    map.insert(253, FieldInfo { name: "egressPhysicalInterface", data_type: DataType::UInt32, description: "The index of a networking device's" });
    map.insert(254, FieldInfo { name: "postDot1qVlanId", data_type: DataType::UInt16, description: "The definition of this Information Element" });
    map.insert(255, FieldInfo { name: "postDot1qCustomerVlanId", data_type: DataType::UInt16, description: "The definition of this Information Element" });
    map.insert(256, FieldInfo { name: "ethernetType", data_type: DataType::UInt16, description: "The Ethernet type field of an" });
    map.insert(257, FieldInfo { name: "postIpPrecedence", data_type: DataType::UInt8, description: "The definition of this Information Element" });
    map.insert(258, FieldInfo { name: "collectionTimeMilliseconds", data_type: DataType::DateTimeMilliseconds, description: "The absolute timestamp at which the" });
    map.insert(259, FieldInfo { name: "exportSctpStreamId", data_type: DataType::UInt16, description: "The value of the SCTP Stream" });
    map.insert(260, FieldInfo { name: "maxExportSeconds", data_type: DataType::DateTimeSeconds, description: "The absolute Export Time of the" });
    map.insert(261, FieldInfo { name: "maxFlowEndSeconds", data_type: DataType::DateTimeSeconds, description: "The latest absolute timestamp of the" });
    map.insert(262, FieldInfo { name: "messageMD5Checksum", data_type: DataType::Binary, description: "The MD5 checksum of the IPFIX" });
    map.insert(263, FieldInfo { name: "messageScope", data_type: DataType::UInt8, description: "The presence of this Information Element" });
    map.insert(264, FieldInfo { name: "minExportSeconds", data_type: DataType::DateTimeSeconds, description: "The absolute Export Time of the" });
    map.insert(265, FieldInfo { name: "minFlowStartSeconds", data_type: DataType::DateTimeSeconds, description: "The earliest absolute timestamp of the" });
    map.insert(266, FieldInfo { name: "opaqueOctets", data_type: DataType::Binary, description: "This Information Element is used to" });
    map.insert(267, FieldInfo { name: "sessionScope", data_type: DataType::UInt8, description: "The presence of this Information Element" });
    map.insert(268, FieldInfo { name: "maxFlowEndMicroseconds", data_type: DataType::DateTimeMicroseconds, description: "The latest absolute timestamp of the" });
    map.insert(269, FieldInfo { name: "maxFlowEndMilliseconds", data_type: DataType::DateTimeMilliseconds, description: "The latest absolute timestamp of the" });
    map.insert(270, FieldInfo { name: "maxFlowEndNanoseconds", data_type: DataType::DateTimeNanoseconds, description: "The latest absolute timestamp of the" });
    map.insert(271, FieldInfo { name: "minFlowStartMicroseconds", data_type: DataType::DateTimeMicroseconds, description: "The earliest absolute timestamp of the" });
    map.insert(272, FieldInfo { name: "minFlowStartMilliseconds", data_type: DataType::DateTimeMilliseconds, description: "The earliest absolute timestamp of the" });
    map.insert(273, FieldInfo { name: "minFlowStartNanoseconds", data_type: DataType::DateTimeNanoseconds, description: "The earliest absolute timestamp of the" });
    map.insert(274, FieldInfo { name: "collectorCertificate", data_type: DataType::Binary, description: "The full X.509 certificate, encoded in" });
    map.insert(275, FieldInfo { name: "exporterCertificate", data_type: DataType::Binary, description: "The full X.509 certificate, encoded in" });
    map.insert(276, FieldInfo { name: "dataRecordsReliability", data_type: DataType::Boolean, description: "The export reliability of Data Records," });
    map.insert(277, FieldInfo { name: "observationPointType", data_type: DataType::UInt8, description: "Type of observation point. Values are" });
    map.insert(278, FieldInfo { name: "newConnectionDeltaCount", data_type: DataType::UInt32, description: "This information element counts the number" });
    map.insert(279, FieldInfo { name: "connectionSumDurationSeconds", data_type: DataType::UInt64, description: "This information element aggregates the total" });
    map.insert(280, FieldInfo { name: "connectionTransactionId", data_type: DataType::UInt64, description: "This information element identifies a transaction" });
    map.insert(281, FieldInfo { name: "postNATSourceIPv6Address", data_type: DataType::Ipv6Address, description: "The definition of this Information Element" });
    map.insert(282, FieldInfo { name: "postNATDestinationIPv6Address", data_type: DataType::Ipv6Address, description: "The definition of this Information Element" });
    map.insert(283, FieldInfo { name: "natPoolId", data_type: DataType::UInt32, description: "Locally unique identifier of a NAT" });
    map.insert(284, FieldInfo { name: "natPoolName", data_type: DataType::String, description: "The name of a NAT pool" });
    map.insert(285, FieldInfo { name: "anonymizationFlags", data_type: DataType::UInt16, description: "A flag word describing specialized modifications" });
    map.insert(286, FieldInfo { name: "anonymizationTechnique", data_type: DataType::UInt16, description: "A description of the anonymization technique" });
    map.insert(287, FieldInfo { name: "informationElementIndex", data_type: DataType::UInt16, description: "A zero-based index of an Information" });
    map.insert(288, FieldInfo { name: "p2pTechnology", data_type: DataType::String, description: "Specifies if the Application ID is" });
    map.insert(289, FieldInfo { name: "tunnelTechnology", data_type: DataType::String, description: "Specifies if the Application ID is" });
    map.insert(290, FieldInfo { name: "encryptedTechnology", data_type: DataType::String, description: "Specifies if the Application ID is" });
    map.insert(291, FieldInfo { name: "basicList", data_type: DataType::String, description: "Specifies a generic Information Element with" });
    map.insert(292, FieldInfo { name: "subTemplateList", data_type: DataType::String, description: "Specifies a generic Information Element with" });
    map.insert(293, FieldInfo { name: "subTemplateMultiList", data_type: DataType::String, description: "Specifies a generic Information Element with" });
    map.insert(294, FieldInfo { name: "bgpValidityState", data_type: DataType::UInt8, description: "This element describes the \"validity state\"" });
    map.insert(295, FieldInfo { name: "IPSecSPI", data_type: DataType::UInt32, description: "IPSec Security Parameters Index (SPI)." });
    map.insert(296, FieldInfo { name: "greKey", data_type: DataType::UInt32, description: "GRE key, which is used for" });
    map.insert(297, FieldInfo { name: "natType", data_type: DataType::UInt8, description: "This Information Element identifies the NAT" });
    map.insert(298, FieldInfo { name: "initiatorPackets", data_type: DataType::UInt64, description: "The total number of layer 4" });
    map.insert(299, FieldInfo { name: "responderPackets", data_type: DataType::UInt64, description: "The total number of layer 4" });
    map.insert(300, FieldInfo { name: "observationDomainName", data_type: DataType::String, description: "The name of an observation domain" });
    map.insert(301, FieldInfo { name: "selectionSequenceId", data_type: DataType::UInt64, description: "From all the packets observed at" });
    map.insert(302, FieldInfo { name: "selectorId", data_type: DataType::UInt64, description: "The Selector ID is the unique" });
    map.insert(303, FieldInfo { name: "informationElementId", data_type: DataType::UInt16, description: "This Information Element contains the ID" });
    map.insert(304, FieldInfo { name: "selectorAlgorithm", data_type: DataType::UInt16, description: "This Information Element identifies the packet" });
    map.insert(305, FieldInfo { name: "samplingPacketInterval", data_type: DataType::UInt32, description: "This Information Element specifies the number" });
    map.insert(306, FieldInfo { name: "samplingPacketSpace", data_type: DataType::UInt32, description: "This Information Element specifies the number" });
    map.insert(307, FieldInfo { name: "samplingTimeInterval", data_type: DataType::UInt32, description: "This Information Element specifies the time" });
    map.insert(308, FieldInfo { name: "samplingTimeSpace", data_type: DataType::UInt32, description: "This Information Element specifies the time" });
    map.insert(309, FieldInfo { name: "samplingSize", data_type: DataType::UInt32, description: "This Information Element specifies the number" });
    map.insert(310, FieldInfo { name: "samplingPopulation", data_type: DataType::UInt32, description: "This Information Element specifies the number" });
    map.insert(311, FieldInfo { name: "samplingProbability", data_type: DataType::Float64, description: "This Information Element specifies the probability" });
    map.insert(312, FieldInfo { name: "dataLinkFrameSize", data_type: DataType::UInt16, description: "This Information Element specifies the length" });
    map.insert(313, FieldInfo { name: "ipHeaderPacketSection", data_type: DataType::Binary, description: "This Information Element carries a series" });
    map.insert(314, FieldInfo { name: "ipPayloadPacketSection", data_type: DataType::Binary, description: "This Information Element carries a series" });
    map.insert(315, FieldInfo { name: "dataLinkFrameSection", data_type: DataType::Binary, description: "This Information Element carries n octets" });
    map.insert(316, FieldInfo { name: "mplsLabelStackSection", data_type: DataType::Binary, description: "This Information Element carries a series" });
    map.insert(317, FieldInfo { name: "mplsPayloadPacketSection", data_type: DataType::Binary, description: "The mplsPayloadPacketSection carries a series of" });
    map.insert(318, FieldInfo { name: "selectorIdTotalPktsObserved", data_type: DataType::UInt64, description: "This Information Element specifies the total" });
    map.insert(319, FieldInfo { name: "selectorIdTotalPktsSelected", data_type: DataType::UInt64, description: "This Information Element specifies the total" });
    map.insert(320, FieldInfo { name: "absoluteError", data_type: DataType::Float64, description: "This Information Element specifies the maximum" });
    map.insert(321, FieldInfo { name: "relativeError", data_type: DataType::Float64, description: "This Information Element specifies the maximum" });
    map.insert(322, FieldInfo { name: "observationTimeSeconds", data_type: DataType::DateTimeSeconds, description: "This Information Element specifies the absolute" });
    map.insert(323, FieldInfo { name: "observationTimeMilliseconds", data_type: DataType::DateTimeMilliseconds, description: "This Information Element specifies the absolute" });
    map.insert(324, FieldInfo { name: "observationTimeMicroseconds", data_type: DataType::DateTimeMicroseconds, description: "This Information Element specifies the absolute" });
    map.insert(325, FieldInfo { name: "observationTimeNanoseconds", data_type: DataType::DateTimeNanoseconds, description: "This Information Element specifies the absolute" });
    map.insert(326, FieldInfo { name: "digestHashValue", data_type: DataType::UInt64, description: "This Information Element specifies the value" });
    map.insert(327, FieldInfo { name: "hashIPPayloadOffset", data_type: DataType::UInt64, description: "This Information Element specifies the IP" });
    map.insert(328, FieldInfo { name: "hashIPPayloadSize", data_type: DataType::UInt64, description: "This Information Element specifies the IP" });
    map.insert(329, FieldInfo { name: "hashOutputRangeMin", data_type: DataType::UInt64, description: "This Information Element specifies the value" });
    map.insert(330, FieldInfo { name: "hashOutputRangeMax", data_type: DataType::UInt64, description: "This Information Element specifies the value" });
    map.insert(331, FieldInfo { name: "hashSelectedRangeMin", data_type: DataType::UInt64, description: "This Information Element specifies the value" });
    map.insert(332, FieldInfo { name: "hashSelectedRangeMax", data_type: DataType::UInt64, description: "This Information Element specifies the value" });
    map.insert(333, FieldInfo { name: "hashDigestOutput", data_type: DataType::Boolean, description: "This Information Element contains a boolean" });
    map.insert(334, FieldInfo { name: "hashInitialiserValue", data_type: DataType::UInt64, description: "This Information Element specifies the initialiser" });
    map.insert(335, FieldInfo { name: "selectorName", data_type: DataType::String, description: "The name of a selector identified" });
    map.insert(336, FieldInfo { name: "upperCILimit", data_type: DataType::Float64, description: "This Information Element specifies the upper" });
    map.insert(337, FieldInfo { name: "lowerCILimit", data_type: DataType::Float64, description: "This Information Element specifies the lower" });
    map.insert(338, FieldInfo { name: "confidenceLevel", data_type: DataType::Float64, description: "This Information Element specifies the confidence" });
    map.insert(339, FieldInfo { name: "informationElementDataType", data_type: DataType::UInt8, description: "A description of the abstract data" });
    map.insert(340, FieldInfo { name: "informationElementDescription", data_type: DataType::String, description: "A UTF-8 [RFC3629] encoded Unicode string" });
    map.insert(341, FieldInfo { name: "informationElementName", data_type: DataType::String, description: "A UTF-8 [RFC3629] encoded Unicode string" });
    map.insert(342, FieldInfo { name: "informationElementRangeBegin", data_type: DataType::UInt64, description: "Contains the inclusive low end of" });
    map.insert(343, FieldInfo { name: "informationElementRangeEnd", data_type: DataType::UInt64, description: "Contains the inclusive high end of" });
    map.insert(344, FieldInfo { name: "informationElementSemantics", data_type: DataType::UInt8, description: "A description of the semantics of" });
    map.insert(345, FieldInfo { name: "informationElementUnits", data_type: DataType::UInt16, description: "A description of the units of" });
    map.insert(346, FieldInfo { name: "observationPointId", data_type: DataType::UInt32, description: "Identifier of the observation point" });
    map.insert(347, FieldInfo { name: "virtualStationInterfaceId", data_type: DataType::Binary, description: "Instance Identifier of the interface to" });
    map.insert(348, FieldInfo { name: "virtualStationInterfaceName", data_type: DataType::String, description: "Name of the interface to a" });
    map.insert(349, FieldInfo { name: "virtualStationUUID", data_type: DataType::Binary, description: "Unique Identifier of a Virtual Station." });
    map.insert(350, FieldInfo { name: "virtualStationName", data_type: DataType::String, description: "Name of a Virtual Station. A" });
    map.insert(351, FieldInfo { name: "layer2SegmentId", data_type: DataType::UInt64, description: "Identifier of a layer 2 network" });
    map.insert(352, FieldInfo { name: "layer2OctetDeltaCount", data_type: DataType::UInt64, description: "The number of layer 2 octets" });
    map.insert(353, FieldInfo { name: "layer2OctetTotalCount", data_type: DataType::UInt64, description: "The total number of layer 2" });
    map.insert(354, FieldInfo { name: "ingressUnicastPacketTotalCount", data_type: DataType::UInt64, description: "The total number of incoming unicast" });
    map.insert(355, FieldInfo { name: "ingressMulticastPacketTotalCount", data_type: DataType::UInt64, description: "The total number of incoming multicast" });
    map.insert(356, FieldInfo { name: "ingressBroadcastPacketTotalCount", data_type: DataType::UInt64, description: "The total number of incoming broadcast" });
    map.insert(357, FieldInfo { name: "egressUnicastPacketTotalCount", data_type: DataType::UInt64, description: "The total number of incoming unicast" });
    map.insert(358, FieldInfo { name: "egressBroadcastPacketTotalCount", data_type: DataType::UInt64, description: "The total number of incoming broadcast" });
    map.insert(359, FieldInfo { name: "monitoringIntervalStartMilliSeconds", data_type: DataType::DateTimeMilliseconds, description: "The absolute timestamp at which the" });
    map.insert(360, FieldInfo { name: "monitoringIntervalEndMilliSeconds", data_type: DataType::DateTimeMilliseconds, description: "The absolute timestamp at which the" });
    map.insert(361, FieldInfo { name: "portRangeStart", data_type: DataType::UInt16, description: "The port number identifying the start" });
    map.insert(362, FieldInfo { name: "portRangeEnd", data_type: DataType::UInt16, description: "The port number identifying the end" });
    map.insert(363, FieldInfo { name: "portRangeStepSize", data_type: DataType::UInt16, description: "The step size in a port" });
    map.insert(364, FieldInfo { name: "portRangeNumPorts", data_type: DataType::UInt16, description: "The number of ports in a" });
    map.insert(365, FieldInfo { name: "staMacAddress", data_type: DataType::MacAddress, description: "The IEEE 802 MAC address of" });
    map.insert(366, FieldInfo { name: "staIPv4Address", data_type: DataType::Ipv4Address, description: "The IPv4 address of a wireless" });
    map.insert(367, FieldInfo { name: "wtpMacAddress", data_type: DataType::MacAddress, description: "The IEEE 802 MAC address of" });
    map.insert(368, FieldInfo { name: "ingressInterfaceType", data_type: DataType::UInt32, description: "The type of interface where packets" });
    map.insert(369, FieldInfo { name: "egressInterfaceType", data_type: DataType::UInt32, description: "The type of interface where packets" });
    map.insert(370, FieldInfo { name: "rtpSequenceNumber", data_type: DataType::UInt16, description: "The RTP sequence number per [RFC3550]." });
    map.insert(371, FieldInfo { name: "userName", data_type: DataType::String, description: "User name associated with the flow." });
    map.insert(372, FieldInfo { name: "applicationCategoryName", data_type: DataType::String, description: "An attribute that provides a first" });
    map.insert(373, FieldInfo { name: "applicationSubCategoryName", data_type: DataType::String, description: "An attribute that provides a second" });
    map.insert(374, FieldInfo { name: "applicationGroupName", data_type: DataType::String, description: "An attribute that groups multiple Application" });
    map.insert(375, FieldInfo { name: "originalFlowsPresent", data_type: DataType::UInt64, description: "The non-conservative count of Original Flows" });
    map.insert(376, FieldInfo { name: "originalFlowsInitiated", data_type: DataType::UInt64, description: "The conservative count of Original Flows" });
    map.insert(377, FieldInfo { name: "originalFlowsCompleted", data_type: DataType::UInt64, description: "The conservative count of Original Flows" });
    map.insert(378, FieldInfo { name: "distinctCountOfSourceIPAddress", data_type: DataType::UInt64, description: "The count of distinct source IP" });
    map.insert(379, FieldInfo { name: "distinctCountOfDestinationIPAddress", data_type: DataType::UInt64, description: "The count of distinct destination IP" });
    map.insert(380, FieldInfo { name: "distinctCountOfSourceIPv4Address", data_type: DataType::UInt32, description: "The count of distinct source IPv4" });
    map.insert(381, FieldInfo { name: "distinctCountOfDestinationIPv4Address", data_type: DataType::UInt32, description: "The count of distinct destination IPv4" });
    map.insert(382, FieldInfo { name: "distinctCountOfSourceIPv6Address", data_type: DataType::UInt64, description: "The count of distinct source IPv6" });
    map.insert(383, FieldInfo { name: "distinctCountOfDestinationIPv6Address", data_type: DataType::UInt64, description: "The count of distinct destination IPv6" });
    map.insert(384, FieldInfo { name: "valueDistributionMethod", data_type: DataType::UInt8, description: "A description of the method used" });
    map.insert(385, FieldInfo { name: "rfc3550JitterMilliseconds", data_type: DataType::UInt32, description: "Interarrival jitter as defined in section" });
    map.insert(386, FieldInfo { name: "rfc3550JitterMicroseconds", data_type: DataType::UInt32, description: "Interarrival jitter as defined in section" });
    map.insert(387, FieldInfo { name: "rfc3550JitterNanoseconds", data_type: DataType::UInt32, description: "Interarrival jitter as defined in section" });
    map.insert(388, FieldInfo { name: "dot1qDEI", data_type: DataType::Boolean, description: "The value of the 1-bit Drop" });
    map.insert(389, FieldInfo { name: "dot1qCustomerDEI", data_type: DataType::Boolean, description: "In case of a QinQ frame," });
    map.insert(390, FieldInfo { name: "flowSelectorAlgorithm", data_type: DataType::UInt16, description: "This Information Element identifies the Intermediate" });
    map.insert(391, FieldInfo { name: "flowSelectedOctetDeltaCount", data_type: DataType::UInt64, description: "This Information Element specifies the volume" });
    map.insert(392, FieldInfo { name: "flowSelectedPacketDeltaCount", data_type: DataType::UInt64, description: "This Information Element specifies the volume" });
    map.insert(393, FieldInfo { name: "flowSelectedFlowDeltaCount", data_type: DataType::UInt64, description: "This Information Element specifies the number" });
    map.insert(394, FieldInfo { name: "selectorIDTotalFlowsObserved", data_type: DataType::UInt64, description: "This Information Element specifies the total" });
    map.insert(395, FieldInfo { name: "selectorIDTotalFlowsSelected", data_type: DataType::UInt64, description: "This Information Element specifies the total" });
    map.insert(396, FieldInfo { name: "samplingFlowInterval", data_type: DataType::UInt64, description: "This Information Element specifies the number" });
    map.insert(397, FieldInfo { name: "samplingFlowSpacing", data_type: DataType::UInt64, description: "This Information Element specifies the number" });
    map.insert(398, FieldInfo { name: "flowSamplingTimeInterval", data_type: DataType::UInt64, description: "This Information Element specifies the time" });
    map.insert(399, FieldInfo { name: "flowSamplingTimeSpacing", data_type: DataType::UInt64, description: "This Information Element specifies the time" });
    map.insert(400, FieldInfo { name: "hashFlowDomain", data_type: DataType::UInt16, description: "This Information Element specifies the Information" });
    map.insert(401, FieldInfo { name: "transportOctetDeltaCount", data_type: DataType::UInt64, description: "The number of octets, excluding IP" });
    map.insert(402, FieldInfo { name: "transportPacketDeltaCount", data_type: DataType::UInt64, description: "The number of packets containing at" });
    map.insert(403, FieldInfo { name: "originalExporterIPv4Address", data_type: DataType::Ipv4Address, description: "The IPv4 address used by the" });
    map.insert(404, FieldInfo { name: "originalExporterIPv6Address", data_type: DataType::Ipv6Address, description: "The IPv6 address used by the" });
    map.insert(405, FieldInfo { name: "originalObservationDomainId", data_type: DataType::UInt32, description: "The Observation Domain ID reported by" });
    map.insert(406, FieldInfo { name: "intermediateProcessId", data_type: DataType::UInt32, description: "Description: An identifier of an Intermediate" });
    map.insert(407, FieldInfo { name: "ignoredDataRecordTotalCount", data_type: DataType::UInt64, description: "Description: The total number of received" });
    map.insert(408, FieldInfo { name: "dataLinkFrameType", data_type: DataType::UInt16, description: "This Information Element specifies the type" });
    map.insert(409, FieldInfo { name: "sectionOffset", data_type: DataType::UInt16, description: "This Information Element specifies the offset" });
    map.insert(410, FieldInfo { name: "sectionExportedOctets", data_type: DataType::UInt16, description: "This Information Element specifies the observed" });
    map.insert(411, FieldInfo { name: "dot1qServiceInstanceTag", data_type: DataType::Binary, description: "This Information Element, which is 16" });
    map.insert(412, FieldInfo { name: "dot1qServiceInstanceId", data_type: DataType::UInt32, description: "The value of the 24-bit Backbone" });
    map.insert(413, FieldInfo { name: "dot1qServiceInstancePriority", data_type: DataType::UInt8, description: "The value of the 3-bit Backbone" });
    map.insert(414, FieldInfo { name: "dot1qCustomerSourceMacAddress", data_type: DataType::MacAddress, description: "The value of the Encapsulated Customer" });
    map.insert(415, FieldInfo { name: "dot1qCustomerDestinationMacAddress", data_type: DataType::MacAddress, description: "The value of the Encapsulated Customer" });
    map.insert(417, FieldInfo { name: "postLayer2OctetDeltaCount", data_type: DataType::UInt64, description: "The definition of this Information Element" });
    map.insert(418, FieldInfo { name: "postMCastLayer2OctetDeltaCount", data_type: DataType::UInt64, description: "The number of layer 2 octets" });
    map.insert(420, FieldInfo { name: "postLayer2OctetTotalCount", data_type: DataType::UInt64, description: "The definition of this Information Element" });
    map.insert(421, FieldInfo { name: "postMCastLayer2OctetTotalCount", data_type: DataType::UInt64, description: "The total number of layer 2" });
    map.insert(422, FieldInfo { name: "minimumLayer2TotalLength", data_type: DataType::UInt64, description: "Layer 2 length of the smallest" });
    map.insert(423, FieldInfo { name: "maximumLayer2TotalLength", data_type: DataType::UInt64, description: "Layer 2 length of the largest" });
    map.insert(424, FieldInfo { name: "droppedLayer2OctetDeltaCount", data_type: DataType::UInt64, description: "The number of layer 2 octets" });
    map.insert(425, FieldInfo { name: "droppedLayer2OctetTotalCount", data_type: DataType::UInt64, description: "The total number of octets in" });
    map.insert(426, FieldInfo { name: "ignoredLayer2OctetTotalCount", data_type: DataType::UInt64, description: "The total number of octets in" });
    map.insert(427, FieldInfo { name: "notSentLayer2OctetTotalCount", data_type: DataType::UInt64, description: "The total number of octets in" });
    map.insert(428, FieldInfo { name: "layer2OctetDeltaSumOfSquares", data_type: DataType::UInt64, description: "The sum of the squared numbers" });
    map.insert(429, FieldInfo { name: "layer2OctetTotalSumOfSquares", data_type: DataType::UInt64, description: "The total sum of the squared" });
    map.insert(430, FieldInfo { name: "layer2FrameDeltaCount", data_type: DataType::UInt64, description: "The number of incoming layer 2" });
    map.insert(431, FieldInfo { name: "layer2FrameTotalCount", data_type: DataType::UInt64, description: "The total number of incoming layer" });
    map.insert(432, FieldInfo { name: "pseudoWireDestinationIPv4Address", data_type: DataType::Ipv4Address, description: "The destination IPv4 address of the" });
    map.insert(433, FieldInfo { name: "ignoredLayer2FrameTotalCount", data_type: DataType::UInt64, description: "The total number of observed layer" });
    map.insert(434, FieldInfo { name: "mibObjectValueInteger", data_type: DataType::String, description: "An IPFIX Information Element that denotes" });
    map.insert(435, FieldInfo { name: "mibObjectValueOctetString", data_type: DataType::Binary, description: "An IPFIX Information Element that denotes" });
    map.insert(436, FieldInfo { name: "mibObjectValueOID", data_type: DataType::Binary, description: "An IPFIX Information Element that denotes" });
    map.insert(437, FieldInfo { name: "mibObjectValueBits", data_type: DataType::Binary, description: "An IPFIX Information Element that denotes" });
    map.insert(438, FieldInfo { name: "mibObjectValueIPAddress", data_type: DataType::Ipv4Address, description: "An IPFIX Information Element that denotes" });
    map.insert(439, FieldInfo { name: "mibObjectValueCounter", data_type: DataType::UInt64, description: "An IPFIX Information Element that denotes" });
    map.insert(440, FieldInfo { name: "mibObjectValueGauge", data_type: DataType::UInt32, description: "An IPFIX Information Element that denotes" });
    map.insert(441, FieldInfo { name: "mibObjectValueTimeTicks", data_type: DataType::UInt32, description: "An IPFIX Information Element that denotes" });
    map.insert(442, FieldInfo { name: "mibObjectValueUnsigned", data_type: DataType::UInt32, description: "An IPFIX Information Element that denotes" });
    map.insert(443, FieldInfo { name: "mibObjectValueTable", data_type: DataType::String, description: "An IPFIX Information Element that denotes" });
    map.insert(444, FieldInfo { name: "mibObjectValueRow", data_type: DataType::String, description: "An IPFIX Information Element that denotes" });
    map.insert(445, FieldInfo { name: "mibObjectIdentifier", data_type: DataType::Binary, description: "An IPFIX Information Element that denotes" });
    map.insert(446, FieldInfo { name: "mibSubIdentifier", data_type: DataType::UInt32, description: "A non-negative sub-identifier of an Object" });
    map.insert(447, FieldInfo { name: "mibIndexIndicator", data_type: DataType::UInt64, description: "A set of bit fields that" });
    map.insert(448, FieldInfo { name: "mibCaptureTimeSemantics", data_type: DataType::UInt8, description: "Indicates when in the lifetime of" });
    map.insert(449, FieldInfo { name: "mibContextEngineID", data_type: DataType::Binary, description: "A mibContextEngineID that specifies the SNMP" });
    map.insert(450, FieldInfo { name: "mibContextName", data_type: DataType::String, description: "An Information Element that denotes that" });
    map.insert(451, FieldInfo { name: "mibObjectName", data_type: DataType::String, description: "The name (called a descriptor in" });
    map.insert(452, FieldInfo { name: "mibObjectDescription", data_type: DataType::String, description: "The value of the DESCRIPTION clause" });
    map.insert(453, FieldInfo { name: "mibObjectSyntax", data_type: DataType::String, description: "The value of the SYNTAX clause" });
    map.insert(454, FieldInfo { name: "mibModuleName", data_type: DataType::String, description: "The textual name of the MIB" });
    map.insert(455, FieldInfo { name: "mobileIMSI", data_type: DataType::String, description: "The International Mobile Subscription Identity (IMSI)." });
    map.insert(456, FieldInfo { name: "mobileMSISDN", data_type: DataType::String, description: "The Mobile Station International Subscriber Directory" });
    map.insert(457, FieldInfo { name: "httpStatusCode", data_type: DataType::UInt16, description: "The HTTP Response Status Code, as" });
    map.insert(458, FieldInfo { name: "sourceTransportPortsLimit", data_type: DataType::UInt16, description: "This Information Element contains the maximum" });
    map.insert(459, FieldInfo { name: "httpRequestMethod", data_type: DataType::String, description: "The HTTP request method, as defined" });
    map.insert(460, FieldInfo { name: "httpRequestHost", data_type: DataType::String, description: "The HTTP request host, as defined" });
    map.insert(461, FieldInfo { name: "httpRequestTarget", data_type: DataType::String, description: "The HTTP request target, as defined" });
    map.insert(462, FieldInfo { name: "httpMessageVersion", data_type: DataType::String, description: "The version of an HTTP/1.1 message" });
    map.insert(463, FieldInfo { name: "natInstanceID", data_type: DataType::UInt32, description: "This Information Element uniquely identifies an" });
    map.insert(464, FieldInfo { name: "internalAddressRealm", data_type: DataType::Binary, description: "This Information Element represents the internal" });
    map.insert(465, FieldInfo { name: "externalAddressRealm", data_type: DataType::Binary, description: "This Information Element represents the external" });
    map.insert(466, FieldInfo { name: "natQuotaExceededEvent", data_type: DataType::UInt32, description: "This Information Element identifies the type" });
    map.insert(467, FieldInfo { name: "natThresholdEvent", data_type: DataType::UInt32, description: "This Information Element identifies a type" });
    map.insert(468, FieldInfo { name: "httpUserAgent", data_type: DataType::String, description: "The HTTP User-Agent header field as" });
    map.insert(469, FieldInfo { name: "httpContentType", data_type: DataType::String, description: "The HTTP Content-Type header field as" });
    map.insert(470, FieldInfo { name: "httpReasonPhrase", data_type: DataType::String, description: "The HTTP reason phrase as defined" });
    map.insert(471, FieldInfo { name: "maxSessionEntries", data_type: DataType::UInt32, description: "This element represents the maximum session" });
    map.insert(472, FieldInfo { name: "maxBIBEntries", data_type: DataType::UInt32, description: "This element represents the maximum BIB" });
    map.insert(473, FieldInfo { name: "maxEntriesPerUser", data_type: DataType::UInt32, description: "This element represents the maximum NAT" });
    map.insert(474, FieldInfo { name: "maxSubscribers", data_type: DataType::UInt32, description: "This element represents the maximum subscribers" });
    map.insert(475, FieldInfo { name: "maxFragmentsPendingReassembly", data_type: DataType::UInt32, description: "This element represents the maximum fragments" });
    map.insert(476, FieldInfo { name: "addressPoolHighThreshold", data_type: DataType::UInt32, description: "This element represents the high threshold" });
    map.insert(477, FieldInfo { name: "addressPoolLowThreshold", data_type: DataType::UInt32, description: "This element represents the low threshold" });
    map.insert(478, FieldInfo { name: "addressPortMappingHighThreshold", data_type: DataType::UInt32, description: "This element represents the high threshold" });
    map.insert(479, FieldInfo { name: "addressPortMappingLowThreshold", data_type: DataType::UInt32, description: "This element represents the low threshold" });
    map.insert(480, FieldInfo { name: "addressPortMappingPerUserHighThreshold", data_type: DataType::UInt32, description: "This element represents the high threshold" });
    map.insert(481, FieldInfo { name: "globalAddressMappingHighThreshold", data_type: DataType::UInt32, description: "This element represents the high threshold" });
    map.insert(482, FieldInfo { name: "vpnIdentifier", data_type: DataType::Binary, description: "VPN ID in the format specified" });
    map.insert(483, FieldInfo { name: "bgpCommunity", data_type: DataType::UInt32, description: "BGP community as defined in [RFC1997]" });
    map.insert(484, FieldInfo { name: "bgpSourceCommunityList", data_type: DataType::String, description: "basicList of zero or more bgpCommunity" });
    map.insert(485, FieldInfo { name: "bgpDestinationCommunityList", data_type: DataType::String, description: "basicList of zero or more bgpCommunity" });
    map.insert(486, FieldInfo { name: "bgpExtendedCommunity", data_type: DataType::Binary, description: "BGP Extended Community as defined in" });
    map.insert(487, FieldInfo { name: "bgpSourceExtendedCommunityList", data_type: DataType::String, description: "basicList of zero or more bgpExtendedCommunity" });
    map.insert(488, FieldInfo { name: "bgpDestinationExtendedCommunityList", data_type: DataType::String, description: "basicList of zero or more bgpExtendedCommunity" });
    map.insert(489, FieldInfo { name: "bgpLargeCommunity", data_type: DataType::Binary, description: "BGP Large Community as defined in" });
    map.insert(490, FieldInfo { name: "bgpSourceLargeCommunityList", data_type: DataType::String, description: "basicList of zero or more bgpLargeCommunity" });
    map.insert(491, FieldInfo { name: "bgpDestinationLargeCommunityList", data_type: DataType::String, description: "basicList of zero or more bgpLargeCommunity" });
    map.insert(492, FieldInfo { name: "srhFlagsIPv6", data_type: DataType::UInt8, description: "The 8-bit Flags field defined in" });
    map.insert(493, FieldInfo { name: "srhTagIPv6", data_type: DataType::UInt16, description: "The 16-bit Tag field defined in" });
    map.insert(494, FieldInfo { name: "srhSegmentIPv6", data_type: DataType::Ipv6Address, description: "The 128-bit IPv6 address that represents" });
    map.insert(495, FieldInfo { name: "srhActiveSegmentIPv6", data_type: DataType::Ipv6Address, description: "The 128-bit IPv6 address that represents" });
    map.insert(496, FieldInfo { name: "srhSegmentIPv6BasicList", data_type: DataType::String, description: "The ordered basicList [RFC6313] of zero" });
    map.insert(497, FieldInfo { name: "srhSegmentIPv6ListSection", data_type: DataType::Binary, description: "The SRv6 Segment List as defined" });
    map.insert(498, FieldInfo { name: "srhSegmentsIPv6Left", data_type: DataType::UInt8, description: "The 8-bit unsigned integer defining the" });
    map.insert(499, FieldInfo { name: "srhIPv6Section", data_type: DataType::Binary, description: "The SRH and its TLVs as" });
    map.insert(500, FieldInfo { name: "srhIPv6ActiveSegmentType", data_type: DataType::UInt8, description: "The designator of the routing protocol" });
    map.insert(501, FieldInfo { name: "srhSegmentIPv6LocatorLength", data_type: DataType::UInt8, description: "The length of the SRH segment" });
    map.insert(502, FieldInfo { name: "srhSegmentIPv6EndpointBehavior", data_type: DataType::UInt16, description: "The 16-bit unsigned integer that represents" });
    map.insert(503, FieldInfo { name: "transportChecksum", data_type: DataType::UInt16, description: "The checksum in the transport header." });
    map.insert(504, FieldInfo { name: "icmpHeaderPacketSection", data_type: DataType::Binary, description: "This Information Element carries a series" });
    map.insert(505, FieldInfo { name: "gtpuFlags", data_type: DataType::UInt8, description: "8-bit flags field indicating the version" });
    map.insert(506, FieldInfo { name: "gtpuMsgType", data_type: DataType::UInt8, description: "8-bit Message type field indicating the" });
    map.insert(507, FieldInfo { name: "gtpuTEid", data_type: DataType::UInt32, description: "32-bit tunnel endpoint identifier field defined" });
    map.insert(508, FieldInfo { name: "gtpuSequenceNum", data_type: DataType::UInt16, description: "16-bit sequence number field defined in" });
    map.insert(509, FieldInfo { name: "gtpuQFI", data_type: DataType::UInt8, description: "6-bit QoS flow identifier field defined" });
    map.insert(510, FieldInfo { name: "gtpuPduType", data_type: DataType::UInt8, description: "4-bit PDU type field defined in" });
    map.insert(511, FieldInfo { name: "bgpSourceAsPathList", data_type: DataType::String, description: "Ordered basicList [RFC6313] of zero or" });
    map.insert(512, FieldInfo { name: "bgpDestinationAsPathList", data_type: DataType::String, description: "Ordered basicList [RFC6313] of zero or" });
    map.insert(513, FieldInfo { name: "ipv6ExtensionHeaderType", data_type: DataType::UInt8, description: "Type of an IPv6 extension header" });
    map.insert(514, FieldInfo { name: "ipv6ExtensionHeaderCount", data_type: DataType::UInt8, description: "The number of consecutive occurrences of" });
    map.insert(515, FieldInfo { name: "ipv6ExtensionHeadersFull", data_type: DataType::String, description: "IPv6 extension headers observed in packets" });
    map.insert(516, FieldInfo { name: "ipv6ExtensionHeaderTypeCountList", data_type: DataType::String, description: "As per Section 4.1 of [RFC8200]," });
    map.insert(517, FieldInfo { name: "ipv6ExtensionHeadersLimit", data_type: DataType::Boolean, description: "When set to \"false\", this IE" });
    map.insert(518, FieldInfo { name: "ipv6ExtensionHeadersChainLength", data_type: DataType::UInt32, description: "In theory, there are no limits" });
    map.insert(519, FieldInfo { name: "ipv6ExtensionHeaderChainLengthList", data_type: DataType::String, description: "This IE is used to report" });
    map.insert(520, FieldInfo { name: "tcpOptionsFull", data_type: DataType::String, description: "TCP options in packets of this" });
    map.insert(521, FieldInfo { name: "tcpSharedOptionExID16", data_type: DataType::UInt16, description: "Reports an observed 2-byte ExID in" });
    map.insert(522, FieldInfo { name: "tcpSharedOptionExID32", data_type: DataType::UInt32, description: "Reports an observed 4-byte ExID in" });
    map.insert(523, FieldInfo { name: "tcpSharedOptionExID16List", data_type: DataType::String, description: "Reports observed 2-byte ExIDs in shared" });
    map.insert(524, FieldInfo { name: "tcpSharedOptionExID32List", data_type: DataType::String, description: "Reports observed 4-byte ExIDs in shared" });
    map.insert(525, FieldInfo { name: "udpSafeOptions", data_type: DataType::String, description: "Observed SAFE UDP options in a" });
    map.insert(526, FieldInfo { name: "udpUnsafeOptions", data_type: DataType::UInt64, description: "Observed UNSAFE UDP options in a" });
    map.insert(527, FieldInfo { name: "udpExID", data_type: DataType::UInt16, description: "Observed ExID in an Experimental option" });
    map.insert(528, FieldInfo { name: "udpSafeExIDList", data_type: DataType::String, description: "Observed ExIDs in the Experimental option" });
    map.insert(529, FieldInfo { name: "udpUnsafeExIDList", data_type: DataType::String, description: "Observed ExIDs in the UNSAFE Experimental" });


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
    
    // Options Template fields (for Silver Peak Template 1024)
    // Note: Some of these conflict with standard IPFIX field definitions
    // but are used differently in Silver Peak Options Templates
    map.insert(341, FieldInfo { name: "lowerCILimit", data_type: DataType::Float64, description: "Statistical confidence interval lower limit" });
    map.insert(344, FieldInfo { name: "dataLinkFrameSize", data_type: DataType::UInt16, description: "Frame size at data link layer" });
    map.insert(345, FieldInfo { name: "dataLinkFrameType", data_type: DataType::UInt16, description: "Type of data link frame" });
    
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
        assert_eq!(parser.max_field_length, 65535);
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
            is_scope: false,
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
            is_scope: false,
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
            is_scope: false,
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
            is_scope: false,
        };
        
        let field_info = parser.get_field_info(&field);
        assert_eq!(field_info.name, "unknown_field_9999");
        assert!(matches!(field_info.data_type, DataType::Binary));
        
        // Unknown enterprise field
        let field = TemplateField {
            field_type: 1001,
            field_length: 4,
            enterprise_number: Some(99999),
            is_scope: false,
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
            is_scope: false,
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
            max_packet_size: 10,
            ..Default::default()
        };
        let parser = FieldParser::new(&config);
        
        let field = TemplateField {
            field_type: 999, // Unknown field, will be parsed as binary
            field_length: 20,
            enterprise_number: None,
            is_scope: false,
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
