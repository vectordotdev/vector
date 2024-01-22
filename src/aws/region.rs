//! Handles the region settings for AWS components.
use aws_types::region::Region;
use vector_lib::configurable::configurable_component;

/// Configuration of the region/endpoint to use when interacting with an AWS service.
#[configurable_component]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[serde(default)]
pub struct RegionOrEndpoint {
    /// The [AWS region][aws_region] of the target service.
    ///
    /// [aws_region]: https://docs.aws.amazon.com/general/latest/gr/rande.html#regional-endpoints
    #[configurable(metadata(docs::examples = "us-east-1"))]
    pub region: Option<String>,

    /// Custom endpoint for use with AWS-compatible services.
    #[configurable(metadata(docs::examples = "http://127.0.0.0:5000/path/to/service"))]
    #[configurable(metadata(docs::advanced))]
    pub endpoint: Option<String>,
}

impl RegionOrEndpoint {
    /// Creates with the given region.
    pub const fn with_region(region: String) -> Self {
        Self {
            region: Some(region),
            endpoint: None,
        }
    }

    /// Creates with both a region and an endpoint.
    pub fn with_both(region: impl Into<String>, endpoint: impl Into<String>) -> Self {
        Self {
            region: Some(region.into()),
            endpoint: Some(endpoint.into()),
        }
    }

    /// Returns the endpoint.
    pub fn endpoint(&self) -> Option<String> {
        self.endpoint.clone()
    }

    /// Returns the region.
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
