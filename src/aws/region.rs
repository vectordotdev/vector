use std::str::FromStr;

use aws_smithy_http::endpoint::Endpoint;
use aws_types::region::Region;
use http::Uri;
use vector_config::configurable_component;

/// Configuration of the region/endpoint to use when interacting with an AWS service.
#[configurable_component]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[serde(default)]
pub struct RegionOrEndpoint {
    /// The AWS region to use.
    pub region: Option<String>,

    /// The API endpoint of the service.
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
        let uri = self.endpoint.as_deref().map(Uri::from_str).transpose()?;
        Ok(uri.map(Endpoint::immutable))
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
