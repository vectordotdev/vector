use aws_smithy_http::endpoint::Endpoint;
use aws_types::region::Region;
use http::Uri;
use serde::{Deserialize, Serialize};
use std::str::FromStr;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct RegionOrEndpoint {
    pub region: String,
    #[serde(default)]
    pub endpoint: Option<String>,
}

impl RegionOrEndpoint {
    pub const fn with_region(region: String) -> Self {
        Self {
            region,
            endpoint: None,
        }
    }

    pub fn with_both(region: impl Into<String>, endpoint: impl Into<String>) -> Self {
        Self {
            region: region.into(),
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

    pub fn region(&self) -> Region {
        Region::new(self.region.clone())
    }
}

#[cfg(test)]
mod tests {
    use indoc::indoc;

    use super::*;

    #[test]
    fn region_required() {
        assert!(toml::from_str::<RegionOrEndpoint>(indoc! {r#"
            endpoint = "http://localhost:8080"
        "#})
        .is_err());

        assert!(toml::from_str::<RegionOrEndpoint>(indoc! {r#"
            endpoint = "http://localhost:8080"
            region = "us-east-1"
        "#})
        .is_ok());
    }

    #[test]
    fn endpoint_optional() {
        assert!(toml::from_str::<RegionOrEndpoint>(indoc! {r#"
            endpoint = "http://localhost:8080"
            region = "us-east-1"
        "#})
        .is_ok());
    }
}
