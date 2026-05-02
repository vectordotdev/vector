//! Configuration for the NetFlow source.

use std::net::SocketAddr;

use vector_lib::configurable::configurable_component;

/// Configuration for the NetFlow source.
#[derive(Clone, Debug)]
#[configurable_component(source("netflow", "Receive NetFlow v5 flow records over UDP."))]
#[serde(deny_unknown_fields)]
pub struct NetflowConfig {
    /// The address to listen for NetFlow packets on.
    #[configurable(metadata(docs::examples = "0.0.0.0:2055"))]
    #[configurable(metadata(docs::examples = "0.0.0.0:4739"))]
    pub address: SocketAddr,

    /// Number of worker tasks to spawn. Each worker binds to the same address when `SO_REUSEPORT`
    /// is supported (Linux, macOS, FreeBSD). Defaults to 1.
    #[configurable(metadata(docs::examples = 1))]
    #[configurable(metadata(docs::examples = 4))]
    #[serde(default = "default_workers")]
    pub workers: usize,

    /// The maximum size of incoming NetFlow packets and field values.
    #[configurable(metadata(docs::type_unit = "bytes"))]
    #[configurable(metadata(docs::examples = 1500))]
    #[configurable(metadata(docs::examples = 65535))]
    #[serde(default = "default_max_packet_size")]
    pub max_packet_size: usize,

    /// Reserved for NetFlow v9 / IPFIX: maximum templates per peer (stub until template support lands).
    #[configurable(metadata(docs::examples = 1000))]
    #[configurable(metadata(docs::examples = 5000))]
    #[serde(default = "default_max_templates")]
    pub max_templates: usize,

    /// Reserved for NetFlow v9 / IPFIX: template cache entry timeout in seconds (used by cleanup stub).
    #[configurable(metadata(docs::type_unit = "seconds"))]
    #[configurable(metadata(docs::examples = 3600))]
    #[configurable(metadata(docs::examples = 7200))]
    #[serde(default = "default_template_timeout")]
    pub template_timeout: u64,

    /// Protocols to accept. This release supports `netflow_v5`.
    #[configurable(metadata(docs::examples = "protocols"))]
    #[configurable(metadata(docs::examples = "netflow_v5"))]
    #[serde(default = "default_protocols")]
    pub protocols: Vec<String>,

    /// Whether to use strict validation for NetFlow v5 records.
    /// When enabled, records that fail validation (e.g. zero packets with non-zero octets) are dropped and counted; per-record details are logged at debug level.
    #[configurable(metadata(docs::examples = true))]
    #[configurable(metadata(docs::examples = false))]
    #[serde(default = "default_strict_validation")]
    pub strict_validation: bool,

    /// Whether to include raw packet bytes (base64-encoded) in each event.
    /// Disabled by default to keep payload size small; enable for debugging or forensics.
    #[configurable(metadata(docs::examples = false))]
    #[configurable(metadata(docs::examples = true))]
    #[serde(default = "default_include_raw_data")]
    pub include_raw_data: bool,
}

// Default value functions
const fn default_max_packet_size() -> usize {
    65535 // UDP max
}

const fn default_max_templates() -> usize {
    1000
}

const fn default_template_timeout() -> u64 {
    1800 // 30 minutes - matches typical resend intervals
}

const fn default_workers() -> usize {
    1
}

fn default_protocols() -> Vec<String> {
    vec!["netflow_v5".to_string()]
}

const fn default_strict_validation() -> bool {
    true
}

const fn default_include_raw_data() -> bool {
    false
}

impl Default for NetflowConfig {
    fn default() -> Self {
        Self {
            address: "0.0.0.0:2055".parse().unwrap(),
            workers: default_workers(),
            max_packet_size: default_max_packet_size(),
            max_templates: default_max_templates(),
            template_timeout: default_template_timeout(),
            protocols: default_protocols(),
            strict_validation: default_strict_validation(),
            include_raw_data: default_include_raw_data(),
        }
    }
}

impl NetflowConfig {
    /// Validate the configuration.
    pub fn validate(&self) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();

        if self.workers == 0 {
            errors.push("workers must be at least 1".to_string());
        }
        if self.max_packet_size == 0 {
            errors.push("max_packet_size must be greater than 0".to_string());
        }
        if self.max_packet_size > 65535 {
            errors.push("max_packet_size cannot exceed 65535 bytes (UDP max)".to_string());
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

        if self.protocols.is_empty() {
            errors.push("at least one protocol must be enabled".to_string());
        }
        const KNOWN_PROTOCOLS: &[&str] = &["netflow_v5"];
        for p in &self.protocols {
            if !KNOWN_PROTOCOLS.contains(&p.as_str()) {
                errors.push(format!(
                    "unknown protocol '{}'; supported: {}",
                    p,
                    KNOWN_PROTOCOLS.join(", ")
                ));
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    /// Check if a specific protocol is enabled.
    pub fn is_protocol_enabled(&self, protocol: &str) -> bool {
        self.protocols.iter().any(|p| p.as_str() == protocol)
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
    fn test_invalid_max_packet_size() {
        let mut config = NetflowConfig::default();
        config.max_packet_size = 0;

        let errors = config.validate().unwrap_err();
        assert!(errors
            .iter()
            .any(|e| e.contains("max_packet_size must be greater than 0")));
    }

    #[test]
    fn test_protocol_enablement() {
        let config = NetflowConfig::default();
        assert!(config.is_protocol_enabled("netflow_v5"));
        assert!(!config.is_protocol_enabled("ipfix"));

        let mut limited_config = NetflowConfig::default();
        limited_config.protocols = vec!["netflow_v5".to_string()];
        assert!(limited_config.is_protocol_enabled("netflow_v5"));
        assert!(!limited_config.is_protocol_enabled("ipfix"));
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
