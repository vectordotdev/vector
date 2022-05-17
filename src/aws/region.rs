use std::str::FromStr;

use aws_smithy_http::endpoint::Endpoint;
use aws_types::region::Region;
use http::Uri;
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

    pub fn with_both(region: impl Into<String>, endpoint: impl Into<String>) -> Self {
        Self {
            region: Some(region.into()),
            endpoint: Some(endpoint.into()),
        }
    }

    pub fn endpoint(&self) -> crate::Result<Option<Endpoint>> {
        if let Some(endpoint) = &self.endpoint {
            Ok(Some(Endpoint::immutable(Uri::from_str(endpoint)?)))
        } else {
            Ok(None)
        }
    }

    pub fn region(&self) -> Option<Region> {
        self.region.clone().map(Region::new)
    }
}

#[cfg(test)]
mod tests {
    use indoc::indoc;

    use super::*;

    #[test]
    fn optional() {
        assert!(toml::from_str::<RegionOrEndpoint>(indoc! {r#"
        "#})
        .is_ok());
    }

    #[test]
    fn region_optional() {
        assert!(toml::from_str::<RegionOrEndpoint>(indoc! {r#"
            endpoint = "http://localhost:8080"
        "#})
        .is_ok());
    }

    #[test]
    fn endpoint_optional() {
        assert!(toml::from_str::<RegionOrEndpoint>(indoc! {r#"
            region = "us-east-1"
        "#})
        .is_ok());
    }
}
