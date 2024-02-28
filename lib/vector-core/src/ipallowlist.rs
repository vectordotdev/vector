use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use vector_config::schema::generate_string_schema;
use vector_config::GenerateError;

use ipnet::IpNet;
use vector_config::{configurable_component, Configurable, ToValue};
use vector_config_common::schema::{SchemaGenerator, SchemaObject};

/// IP network allowlist settings for network components
#[configurable_component]
#[derive(Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields, transparent)]
#[configurable(metadata(docs::human_name = "Allowed IP network origins"))]
pub struct IpAllowlistConfig(pub Vec<IpNetConfig>);

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
        Ok(generate_string_schema())
    }
}
