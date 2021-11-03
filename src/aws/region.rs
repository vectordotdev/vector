use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct RegionOrEndpoint {
    pub region: Option<String>,
    pub endpoint: Option<String>,
}

impl RegionOrEndpoint {
    pub const fn with_region(region: String) -> Self {
        Self {
            region: Some(region),
            endpoint: None,
        }
    }

    pub const fn with_endpoint(endpoint: String) -> Self {
        Self {
            region: None,
            endpoint: Some(endpoint),
        }
    }
}
