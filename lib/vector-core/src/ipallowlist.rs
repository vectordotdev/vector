use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use vector_config::GenerateError;

use ipnet::IpNet;
use vector_config::{configurable_component, Configurable, Metadata, ToValue};
use vector_config_common::schema::{InstanceType, SchemaGenerator, SchemaObject};

/// List of allowed origin IP networks.
#[configurable_component]
#[derive(Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields, transparent)]
#[configurable(metadata(docs::human_name = "Allowed IP network origins"))]
pub struct IpAllowlistConfig(pub Vec<IpNetConfig>);

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
