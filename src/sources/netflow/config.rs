//! Configuration for the NetFlow source.

use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use vector_lib::configurable::configurable_component;
use crate::serde::default_true;


/// Configuration for the NetFlow source.
#[derive(Clone, Debug)]
#[configurable_component(source("netflow"))]
#[serde(deny_unknown_fields)]
pub struct NetflowConfig {
    /// The address to listen for NetFlow packets on.
    #[configurable(metadata(docs::examples = "0.0.0.0:2055"))]
    #[configurable(metadata(docs::examples = "0.0.0.0:4739"))]
    pub address: SocketAddr,

    /// The maximum size of incoming NetFlow packets.
    #[configurable(metadata(docs::type_unit = "bytes"))]
    #[configurable(metadata(docs::examples = 1500))]
    #[configurable(metadata(docs::examples = 9000))]
    pub max_length: usize,

    /// The maximum length of field values before truncation.
    #[configurable(metadata(docs::type_unit = "bytes"))]
    #[configurable(metadata(docs::examples = 1024))]
    #[configurable(metadata(docs::examples = 4096))]
    pub max_field_length: usize,

    /// The maximum number of templates to cache per peer.
    #[configurable(metadata(docs::examples = 1000))]
    #[configurable(metadata(docs::examples = 5000))]
    pub max_templates: usize,

    /// The timeout for template cache entries in seconds.
    #[configurable(metadata(docs::type_unit = "seconds"))]
    #[configurable(metadata(docs::examples = 3600))]
    #[configurable(metadata(docs::examples = 7200))]
    pub template_timeout: u64,

    /// The maximum size of a single NetFlow message in bytes.
    #[configurable(metadata(docs::type_unit = "bytes"))]
    #[configurable(metadata(docs::examples = 65536))]
    #[configurable(metadata(docs::examples = 1048576))]
    pub max_message_size: usize,

    /// Protocols to accept (netflow_v5, netflow_v9, ipfix, sflow).
    #[configurable(metadata(docs::examples = "protocols"))]
    #[configurable(metadata(docs::examples = "netflow_v5"))]
    #[configurable(metadata(docs::examples = "netflow_v9"))]
    #[configurable(metadata(docs::examples = "ipfix"))]
    #[configurable(metadata(docs::examples = "sflow"))]
    pub protocols: Vec<String>,

    /// Whether to parse enterprise fields.
    #[configurable(metadata(docs::examples = false))]
    #[configurable(metadata(docs::examples = true))]
    pub parse_enterprise_fields: bool,

    /// Whether to parse options templates.
    #[configurable(metadata(docs::examples = false))]
    #[configurable(metadata(docs::examples = true))]
    pub parse_options_templates: bool,

    /// Whether to parse variable length fields.
    #[configurable(metadata(docs::examples = true))]
    #[configurable(metadata(docs::examples = false))]
    pub parse_variable_length_fields: bool,

    /// Custom enterprise fields.
    #[serde(default)]
    pub enterprise_fields: std::collections::HashMap<String, String>, // Simplified for now

    /// Whether to buffer data records while waiting for templates.
    /// When enabled, data records without templates are buffered for up to `template_timeout` seconds.
    #[configurable(metadata(docs::examples = true))]
    #[configurable(metadata(docs::examples = false))]
    pub buffer_missing_templates: bool,

    /// Maximum number of data records to buffer per template while waiting for template definition.
    #[configurable(metadata(docs::examples = 100))]
    #[configurable(metadata(docs::examples = 1000))]
    pub max_buffered_records: usize,

    /// How to handle Options Template data records (exporter metadata).
    /// Options: "emit" (emit as separate events), "discard" (ignore), "enrich" (use for enrichment only)
    #[configurable(metadata(docs::examples = "emit"))]
    #[configurable(metadata(docs::examples = "discard"))]
    #[configurable(metadata(docs::examples = "enrich"))]
    pub options_template_handling: String,
}

/// Supported flow protocols.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FlowProtocol {
    /// NetFlow version 5 (legacy, fixed format)
    NetflowV5,
    /// NetFlow version 9 (template-based)
    NetflowV9,
    /// IPFIX (Internet Protocol Flow Information Export)
    Ipfix,
    /// sFlow (sampled flow)
    Sflow,
}

/// Configuration for enterprise-specific fields.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct EnterpriseFieldConfig {
    /// Human-readable name for the field.
    pub name: String,
    /// Data type for parsing the field value.
    pub field_type: FieldType,
    /// Optional description of the field.
    pub description: Option<String>,
}

/// Supported field data types for enterprise fields.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FieldType {
    /// 8-bit unsigned integer
    Uint8,
    /// 16-bit unsigned integer
    Uint16,
    /// 32-bit unsigned integer
    Uint32,
    /// 64-bit unsigned integer
    Uint64,
    /// IPv4 address (4 bytes)
    Ipv4Address,
    /// IPv6 address (16 bytes)
    Ipv6Address,
    /// MAC address (6 bytes)
    MacAddress,
    /// UTF-8 string
    String,
    /// Raw binary data (base64 encoded)
    Binary,
    /// Boolean value
    Boolean,
    /// 32-bit floating point
    Float32,
    /// 64-bit floating point
    Float64,
}

// Default value functions
const fn default_max_length() -> usize {
    1500
}

const fn default_max_field_length() -> usize {
    1024
}

const fn default_max_templates() -> usize {
    1000
}

const fn default_template_timeout() -> u64 {
    3600 // 1 hour
}

const fn default_max_message_size() -> usize {
    65536
}

impl Default for NetflowConfig {
    fn default() -> Self {
        Self {
            address: "0.0.0.0:2055".parse().unwrap(),
            max_length: default_max_length(),
            max_field_length: default_max_field_length(),
            max_templates: default_max_templates(),
            template_timeout: default_template_timeout(),
            max_message_size: default_max_message_size(),
            protocols: vec!["netflow_v5".to_string(), "netflow_v9".to_string(), "ipfix".to_string(), "sflow".to_string()],
            parse_enterprise_fields: default_true(),
            parse_options_templates: default_true(),
            parse_variable_length_fields: default_true(),
            enterprise_fields: std::collections::HashMap::new(),
            buffer_missing_templates: true,
            max_buffered_records: 100,
            options_template_handling: "discard".to_string(),
        }
    }
}

impl NetflowConfig {
    /// Validate the configuration.
    pub fn validate(&self) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();

        // Validate numeric ranges
        if self.max_length == 0 {
            errors.push("max_length must be greater than 0".to_string());
        }
        if self.max_length > 65535 {
            errors.push("max_length cannot exceed 65535 bytes".to_string());
        }

        if self.max_field_length == 0 {
            errors.push("max_field_length must be greater than 0".to_string());
        }

        if self.max_templates == 0 {
            errors.push("max_templates must be greater than 0".to_string());
        }
        if self.max_templates > 100_000 {
            errors.push("max_templates cannot exceed 100,000 (memory usage)".to_string());
        }

        if self.template_timeout == 0 {
            errors.push("template_timeout must be greater than 0".to_string());
        }

        if self.max_buffered_records == 0 {
            errors.push("max_buffered_records must be greater than 0".to_string());
        }
        if self.max_buffered_records > 10000 {
            errors.push("max_buffered_records cannot exceed 10,000 (memory usage)".to_string());
        }

        // Validate protocols list
        if self.protocols.is_empty() {
            errors.push("at least one protocol must be enabled".to_string());
        }



        // Validate enterprise field mappings
        for (key, field_name) in &self.enterprise_fields {
            if !key.contains(':') {
                errors.push(format!(
                    "enterprise field key '{}' must be in format 'enterprise_id:field_id'",
                    key
                ));
                continue;
            }

            let parts: Vec<&str> = key.split(':').collect();
            if parts.len() != 2 {
                errors.push(format!(
                    "enterprise field key '{}' must have exactly one colon",
                    key
                ));
                continue;
            }

            if parts[0].parse::<u32>().is_err() {
                errors.push(format!(
                    "enterprise field key '{}' has invalid enterprise_id (must be a number)",
                    key
                ));
            }

            if parts[1].parse::<u16>().is_err() {
                errors.push(format!(
                    "enterprise field key '{}' has invalid field_id (must be a number)",
                    key
                ));
            }

            if field_name.is_empty() {
                errors.push(format!(
                    "enterprise field '{}' must have a non-empty name",
                    key
                ));
            }

            // Validate field name doesn't conflict with standard fields
            if field_name.starts_with("vector_") {
                errors.push(format!(
                    "enterprise field name '{}' cannot start with 'vector_' (reserved)",
                    field_name
                ));
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    /// Get the enterprise field name for a specific enterprise and field ID.
    pub fn get_enterprise_field(&self, enterprise_id: u32, field_id: u16) -> Option<&String> {
        let key = format!("{}:{}", enterprise_id, field_id);
        self.enterprise_fields.get(&key)
    }

    /// Check if a specific protocol is enabled.
    pub fn is_protocol_enabled(&self, protocol: &str) -> bool {
        self.protocols.contains(&protocol.to_string())
    }
}

impl crate::config::GenerateConfig for NetflowConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(Self::default()).unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config_validation() {
        let config = NetflowConfig::default();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_invalid_max_length() {
        let mut config = NetflowConfig::default();
        config.max_length = 0;
        
        let errors = config.validate().unwrap_err();
        assert!(errors.iter().any(|e| e.contains("max_length must be greater than 0")));
    }

    #[test]
    fn test_multicast_address_validation() {
        // This test is no longer relevant since we removed multicast_groups
        // Keeping the test structure for future use
        let config = NetflowConfig::default();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_enterprise_field_validation() {
        let mut config = NetflowConfig::default();
        
        // Valid enterprise field
        config.enterprise_fields.insert(
            "9:1001".to_string(),
            "cisco_app_id".to_string(),
        );
        assert!(config.validate().is_ok());
        
        // Invalid key format
        config.enterprise_fields.insert(
            "invalid_key".to_string(),
            "test".to_string(),
        );
        
        let errors = config.validate().unwrap_err();
        assert!(errors.iter().any(|e| e.contains("must be in format 'enterprise_id:field_id'")));
    }

    #[test]
    fn test_protocol_enablement() {
        let config = NetflowConfig::default();
        assert!(config.is_protocol_enabled("netflow_v5"));
        assert!(config.is_protocol_enabled("ipfix"));
        
        let mut limited_config = NetflowConfig::default();
        limited_config.protocols = vec!["netflow_v5".to_string()];
        assert!(limited_config.is_protocol_enabled("netflow_v5"));
        assert!(!limited_config.is_protocol_enabled("ipfix"));
    }

    #[test]
    fn test_enterprise_field_lookup() {
        let mut config = NetflowConfig::default();
        config.enterprise_fields.insert(
            "9:1001".to_string(),
            "cisco_app_id".to_string(),
        );
        
        let field = config.get_enterprise_field(9, 1001);
        assert!(field.is_some());
        assert_eq!(field.unwrap(), "cisco_app_id");
        
        let missing = config.get_enterprise_field(9, 1002);
        assert!(missing.is_none());
    }

    #[test]
    fn test_config_serialization() {
        let config = NetflowConfig::default();
        let toml_value = toml::Value::try_from(&config).unwrap();
        assert!(toml_value.is_table());
        
        let serialized = toml::to_string(&config).unwrap();
        let deserialized: NetflowConfig = toml::from_str(&serialized).unwrap();
        assert_eq!(config.address, deserialized.address);
        assert_eq!(config.protocols, deserialized.protocols);
    }
}