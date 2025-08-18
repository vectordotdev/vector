use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use vector_config::GenerateError;

use ipnet::IpNet;
use vector_config::{configurable_component, Configurable, Metadata, ToValue};
use vector_config_common::schema::{InstanceType, SchemaGenerator, SchemaObject};

/// List of allowed origin IP networks. IP addresses must be in CIDR notation.
#[configurable_component]
#[derive(Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields, transparent)]
#[configurable(metadata(docs::human_name = "Allowed IP network origins"))]
#[configurable(metadata(docs::examples = "ip_allow_list_example()"))]
pub struct IpAllowlistConfig(pub Vec<IpNetConfig>);

const fn ip_allow_list_example() -> [&'static str; 4] {
    [
        "192.168.0.0/16",
        "127.0.0.1/32",
        "::1/128",
        "9876:9ca3:99ab::23/128",
    ]
}

/// IP network
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, transparent)]
pub struct IpNetConfig(pub IpNet);

impl ToValue for IpNetConfig {
    fn to_value(&self) -> serde_json::Value {
        serde_json::Value::String(self.0.to_string())
    }
}

impl Configurable for IpNetConfig {
    fn generate_schema(
        _: &RefCell<SchemaGenerator>,
    ) -> std::result::Result<SchemaObject, GenerateError> {
        Ok(SchemaObject {
            instance_type: Some(InstanceType::String.into()),
            ..Default::default()
        })
    }

    fn metadata() -> Metadata {
        Metadata::with_description("IP network")
    }
}

impl From<IpAllowlistConfig> for Vec<IpNet> {
    fn from(value: IpAllowlistConfig) -> Self {
        value.0.iter().map(|net| net.0).collect()
    }
}
