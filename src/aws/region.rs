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

    /// Whether to use [FIPS-compliant endpoints][fips] when communicating with AWS services.
    ///
    /// When enabled, the SDK resolves FIPS-compliant endpoints for the target service.
    /// Using FIPS-compliant endpoints is required for FedRAMP and other compliance environments. When omitted, the
    /// SDK falls back to its default provider chain (the `AWS_USE_FIPS_ENDPOINT` environment
    /// variable and AWS config files).
    ///
    /// [fips]: https://docs.aws.amazon.com/sdkref/latest/guide/setting-global-aws_use_fips_endpoint.html
    #[configurable(metadata(docs::advanced))]
    pub use_fips_endpoint: Option<bool>,
}

impl RegionOrEndpoint {
    /// Creates with the given region.
    pub const fn with_region(region: String) -> Self {
        Self {
            region: Some(region),
            endpoint: None,
            use_fips_endpoint: None,
        }
    }

    /// Creates with both a region and an endpoint.
    pub fn with_both(region: impl Into<String>, endpoint: impl Into<String>) -> Self {
        Self {
            region: Some(region.into()),
            endpoint: Some(endpoint.into()),
            use_fips_endpoint: None,
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

    /// Returns the FIPS endpoint setting.
    pub const fn use_fips_endpoint(&self) -> Option<bool> {
        self.use_fips_endpoint
    }
}

#[cfg(test)]
mod tests {
    use indoc::indoc;

    use super::*;

    #[test]
    fn optional() {
        assert!(
            toml::from_str::<RegionOrEndpoint>(indoc! {"
            "})
            .is_ok()
        );
    }

    #[test]
    fn region_optional() {
        assert!(
            toml::from_str::<RegionOrEndpoint>(indoc! {r#"
            endpoint = "http://localhost:8080"
        "#})
            .is_ok()
        );
    }

    #[test]
    fn endpoint_optional() {
        assert!(
            toml::from_str::<RegionOrEndpoint>(indoc! {r#"
            region = "us-east-1"
        "#})
            .is_ok()
        );
    }

    #[test]
    fn use_fips_endpoint() {
        let config: RegionOrEndpoint = toml::from_str(indoc! {r#"
            region = "us-east-1"
            use_fips_endpoint = true
        "#})
        .unwrap();
        assert_eq!(config.use_fips_endpoint(), Some(true));
    }

    #[test]
    fn use_fips_endpoint_optional() {
        let config: RegionOrEndpoint = toml::from_str(indoc! {r#"
            region = "us-east-1"
        "#})
        .unwrap();
        assert_eq!(config.use_fips_endpoint(), None);
    }
}
