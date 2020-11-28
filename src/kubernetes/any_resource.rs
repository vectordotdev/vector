//! A generic interface for K8s resources.

use k8s_openapi::{Metadata, Resource};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::{sync::Arc, unimplemented};

/// Trait to apply to Any resource returned from k8s API
pub trait AnyResource: Metadata + DeserializeOwned {}

/// Abstract an arbitrary Json Response from K8s API
#[derive(Debug, Serialize, Deserialize)]
pub struct Json {
    #[serde(flatten)]
    value: Arc<serde_json::Value>,
}

pub struct JsonObjectMeta {
    value: Arc<serde_json::Value>,
}

impl Resource for Json {
    // TODO: these are temprary, remove before the merge.
    const API_VERSION: &'static str = "dummy";
    const GROUP: &'static str = "dummy";
    const KIND: &'static str = "dummy";
    const VERSION: &'static str = "dummy";
}

impl Metadata for Json {
    type Ty = JsonObjectMeta;
    fn metadata(&self) -> &<Self as Metadata>::Ty {
        unimplemented!();
    }

    fn metadata_mut(&mut self) -> &mut <Self as Metadata>::Ty {
        unimplemented!();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_RESOURCE_1: &str = include_str!("sample_resource_1.json");

    #[test]
    fn test_deserialize() {
        // This will fail - we haven't implemented appropriate deserialization yet
        let parsed: Json = serde_json::from_str(SAMPLE_RESOURCE_1).expect("parsing failed");
        let name = &parsed.value["metadata"]["name"];
        assert_eq!(name, "myapp-qwert");
    }
}
