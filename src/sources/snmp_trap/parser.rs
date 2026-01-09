use bytes::Bytes;
use chrono::Utc;
use serde_json::json;
use smallvec::{SmallVec, smallvec};
use snmp_parser::{
    parse_snmp_v1, parse_snmp_v2c,
    snmp::{SnmpMessage, SnmpPdu},
};
use std::net::SocketAddr;
use vector_lib::{
    config::log_schema,
    event::{Event, LogEvent},
};

/// Error types for SNMP trap parsing
#[derive(Debug)]
pub enum ParseError {
    SnmpParseError(String),
    InvalidPduType,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParseError::SnmpParseError(msg) => write!(f, "Failed to parse SNMP message: {}", msg),
            ParseError::InvalidPduType => write!(f, "Invalid PDU type for trap"),
        }
    }
}

impl std::error::Error for ParseError {}

/// Parse an SNMP trap from raw bytes and convert to Vector log event
pub fn parse_snmp_trap(
    data: &Bytes,
    source_addr: SocketAddr,
) -> Result<SmallVec<[Event; 1]>, ParseError> {
    // Try parsing as SNMPv1 first
    match parse_snmp_v1(data) {
        Ok((_, message)) => {
            if message.version == 0 {
                // SNMPv1 (version field is 0)
                return parse_v1_trap(message, source_addr);
            }
        }
        Err(_) => {
            // Not v1, try v2c
        }
    }

    // Try parsing as SNMPv2c
    match parse_snmp_v2c(data) {
        Ok((_, message)) => {
            if message.version == 1 {
                // SNMPv2c (version field is 1)
                return parse_v2c_trap(message, source_addr);
            }
        }
        Err(e) => {
            return Err(ParseError::SnmpParseError(format!("{:?}", e)));
        }
    }

    Err(ParseError::SnmpParseError(
        "Could not parse as v1 or v2c".to_string(),
    ))
}

fn parse_v1_trap(
    message: SnmpMessage,
    source_addr: SocketAddr,
) -> Result<SmallVec<[Event; 1]>, ParseError> {
    match message.pdu {
        SnmpPdu::TrapV1(trap) => {
            let mut log = LogEvent::default();

            // Add timestamp
            if let Some(timestamp_key) = log_schema().timestamp_key() {
                log.insert((vector_lib::lookup::PathPrefix::Event, timestamp_key), Utc::now());
            }

            // Basic fields
            log.insert("snmp_version", "1");
            log.insert("source_address", source_addr.to_string());
            log.insert("community", String::from_utf8_lossy(message.community.as_bytes()).to_string());

            // Trap-specific fields
            log.insert("enterprise_oid", trap.enterprise.to_string());
            
            // Format agent address
            let agent_addr_str = match trap.agent_addr {
                snmp_parser::snmp::NetworkAddress::IPv4(ip) => {
                    let octets = ip.octets();
                    format!("{}.{}.{}.{}", octets[0], octets[1], octets[2], octets[3])
                }
            };
            log.insert("agent_address", agent_addr_str);
            
            log.insert("generic_trap", trap.generic_trap.0 as i64);
            log.insert("specific_trap", trap.specific_trap as i64);
            log.insert("uptime", trap.timestamp as i64);

            // Parse varbinds
            let mut varbinds = Vec::new();
            for var in &trap.var {
                let oid = var.oid.to_string();
                let value = match &var.val {
                    snmp_parser::snmp::VarBindValue::Value(v) => format_object_value(v),
                    snmp_parser::snmp::VarBindValue::Unspecified => "unspecified".to_string(),
                    snmp_parser::snmp::VarBindValue::NoSuchObject => "noSuchObject".to_string(),
                    snmp_parser::snmp::VarBindValue::NoSuchInstance => "noSuchInstance".to_string(),
                    snmp_parser::snmp::VarBindValue::EndOfMibView => "endOfMibView".to_string(),
                };
                varbinds.push(json!({
                    "oid": oid,
                    "value": value,
                }));
            }
            log.insert("varbinds", varbinds);

            // Add a human-readable message
            let trap_type = match trap.generic_trap.0 {
                0 => "coldStart",
                1 => "warmStart",
                2 => "linkDown",
                3 => "linkUp",
                4 => "authenticationFailure",
                5 => "egpNeighborLoss",
                6 => "enterpriseSpecific",
                _ => "unknown",
            };
            log.insert(
                "message",
                format!(
                    "SNMPv1 trap from {} ({}): {}",
                    source_addr, trap.enterprise, trap_type
                ),
            );

            Ok(smallvec![Event::Log(log)])
        }
        _ => Err(ParseError::InvalidPduType),
    }
}

fn parse_v2c_trap(
    message: SnmpMessage,
    source_addr: SocketAddr,
) -> Result<SmallVec<[Event; 1]>, ParseError> {
    match message.pdu {
        SnmpPdu::Generic(pdu) => {
            let mut log = LogEvent::default();

            // Add timestamp
            if let Some(timestamp_key) = log_schema().timestamp_key() {
                log.insert((vector_lib::lookup::PathPrefix::Event, timestamp_key), Utc::now());
            }

            // Basic fields
            log.insert("snmp_version", "2c");
            log.insert("source_address", source_addr.to_string());
            log.insert("community", String::from_utf8_lossy(message.community.as_bytes()).to_string());

            // SNMPv2 traps include request_id
            log.insert("request_id", pdu.req_id as i64);

            // Parse varbinds to extract sysUpTime and snmpTrapOID
            let mut varbinds = Vec::new();
            let mut uptime = None;
            let mut trap_oid = None;

            for var in &pdu.var {
                let oid = var.oid.to_string();
                let value = match &var.val {
                    snmp_parser::snmp::VarBindValue::Value(v) => format_object_value(&v),
                    snmp_parser::snmp::VarBindValue::Unspecified => "unspecified".to_string(),
                    snmp_parser::snmp::VarBindValue::NoSuchObject => "noSuchObject".to_string(),
                    snmp_parser::snmp::VarBindValue::NoSuchInstance => "noSuchInstance".to_string(),
                    snmp_parser::snmp::VarBindValue::EndOfMibView => "endOfMibView".to_string(),
                };

                // sysUpTime is typically the first varbind (OID 1.3.6.1.2.1.1.3.0)
                if oid.starts_with("1.3.6.1.2.1.1.3") {
                    uptime = Some(value.clone());
                }

                // snmpTrapOID is typically the second varbind (OID 1.3.6.1.6.3.1.1.4.1.0)
                if oid.starts_with("1.3.6.1.6.3.1.1.4.1") {
                    trap_oid = Some(value.clone());
                }

                varbinds.push(json!({
                    "oid": oid,
                    "value": value,
                }));
            }

            if let Some(uptime_val) = uptime {
                log.insert("uptime", uptime_val);
            }

            if let Some(trap_oid_val) = &trap_oid {
                log.insert("trap_oid", trap_oid_val.clone());
            }

            log.insert("varbinds", varbinds);

            // Add a human-readable message
            let trap_desc = trap_oid
                .as_ref()
                .map(|o| o.as_str())
                .unwrap_or("unknown");
            log.insert(
                "message",
                format!("SNMPv2c trap from {}: {}", source_addr, trap_desc),
            );

            Ok(smallvec![Event::Log(log)])
        }
        _ => Err(ParseError::InvalidPduType),
    }
}

/// Format an SNMP object value as a string
fn format_object_value(val: &snmp_parser::snmp::ObjectSyntax) -> String {
    use snmp_parser::snmp::{NetworkAddress, ObjectSyntax};

    match val {
        ObjectSyntax::Number(n) => n.to_string(),
        ObjectSyntax::String(s) => String::from_utf8_lossy(s).to_string(),
        ObjectSyntax::Object(oid) => oid.to_string(),
        ObjectSyntax::BitString(bits) => format!("BitString({:?})", bits),
        ObjectSyntax::IpAddress(net_addr) => match net_addr {
            NetworkAddress::IPv4(ip) => {
                let octets = ip.octets();
                format!("{}.{}.{}.{}", octets[0], octets[1], octets[2], octets[3])
            }
        },
        ObjectSyntax::Counter32(c) => c.to_string(),
        ObjectSyntax::Gauge32(g) => g.to_string(),
        ObjectSyntax::TimeTicks(t) => t.to_string(),
        ObjectSyntax::Opaque(o) => format!("Opaque({:?})", o),
        ObjectSyntax::Counter64(c) => c.to_string(),
        ObjectSyntax::UInteger32(u) => u.to_string(),
        // Handle any other variants
        _ => format!("{:?}", val),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr};

    #[test]
    fn test_format_object_value() {
        use snmp_parser::snmp::{NetworkAddress, ObjectSyntax};

        assert_eq!(format_object_value(&ObjectSyntax::Number(42)), "42");
        assert_eq!(
            format_object_value(&ObjectSyntax::String(b"test")),
            "test"
        );
        assert_eq!(format_object_value(&ObjectSyntax::Counter32(100)), "100");
        assert_eq!(format_object_value(&ObjectSyntax::Gauge32(200)), "200");
        assert_eq!(format_object_value(&ObjectSyntax::TimeTicks(300)), "300");
        assert_eq!(
            format_object_value(&ObjectSyntax::IpAddress(NetworkAddress::IPv4(
                Ipv4Addr::new(192, 168, 1, 1)
            ))),
            "192.168.1.1"
        );
    }

    #[test]
    fn test_parse_invalid_data() {
        let data = Bytes::from("invalid data");
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);
        let result = parse_snmp_trap(&data, addr);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_empty_data() {
        let data = Bytes::from("");
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);
        let result = parse_snmp_trap(&data, addr);
        assert!(result.is_err());
    }
}


