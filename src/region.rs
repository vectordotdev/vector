#![cfg(feature = "rusoto_core")]

use http::{uri::InvalidUri, Uri};
use rusoto_core::{region::ParseRegionError, Region};
use serde::{Deserialize, Serialize};
use snafu::{ResultExt, Snafu};
use std::convert::TryFrom;
use std::str::FromStr;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct RegionOrEndpoint {
    region: Option<String>,
    endpoint: Option<String>,
}

impl RegionOrEndpoint {
    pub fn with_region(region: String) -> Self {
        Self {
            region: Some(region),
            endpoint: None,
        }
    }

    pub fn with_endpoint(endpoint: String) -> Self {
        Self {
            region: None,
            endpoint: Some(endpoint),
        }
    }
}

#[derive(Debug, Snafu)]
pub enum ParseError {
    #[snafu(display("Failed to parse custom endpoint as URI: {}", source))]
    EndpointParseError { source: InvalidUri },
    #[snafu(display("{}", source))]
    RegionParseError { source: ParseRegionError },
    #[snafu(display("Only one of 'region' or 'endpoint' can be specified"))]
    BothRegionAndEndpoint,
    #[snafu(display("Must set either 'region' or 'endpoint'"))]
    MissingRegionAndEndpoint,
}

impl TryFrom<&RegionOrEndpoint> for Region {
    type Error = ParseError;

    fn try_from(r: &RegionOrEndpoint) -> Result<Self, Self::Error> {
        match (&r.region, &r.endpoint) {
            (Some(region), None) => region.parse().context(RegionParseError),
            (None, Some(endpoint)) => region_from_endpoint(endpoint),
            (Some(_), Some(_)) => Err(ParseError::BothRegionAndEndpoint),
            (None, None) => Err(ParseError::MissingRegionAndEndpoint),
        }
    }
}

impl TryFrom<RegionOrEndpoint> for Region {
    type Error = ParseError;
    fn try_from(r: RegionOrEndpoint) -> Result<Self, Self::Error> {
        Region::try_from(&r)
    }
}

/// Translate an endpoint URL into a Region
pub fn region_from_endpoint(endpoint: &str) -> Result<Region, ParseError> {
    let uri = endpoint.parse::<Uri>().context(EndpointParseError)?;
    let name = region_name_from_host(uri.host().unwrap_or(""));

    // Reconstitute the endpoint from the URI, but strip off all path components
    let pq_len = uri
        .path_and_query()
        .map(|pq| pq.as_str().len())
        .unwrap_or(1);
    let endpoint = uri.to_string();
    let endpoint = endpoint[..endpoint.len() - pq_len].to_string();

    Ok(Region::Custom { name, endpoint })
}

/// Translate a hostname into a custom region name
fn region_name_from_host(host: &str) -> String {
    // Find the first part of the domain name that matches a known region
    for part in host.split('.') {
        if let Ok(region) = Region::from_str(part) {
            return region.name().into();
        }
    }
    // Couldn't find a valid region, use the default
    "custom".into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusoto_core::Region;
    use serde::Deserialize;
    use std::convert::TryInto;

    #[derive(Deserialize)]
    struct Config {
        inner: Inner,
    }

    #[derive(Deserialize)]
    struct Inner {
        #[serde(flatten)]
        region: RegionOrEndpoint,
    }

    #[test]
    fn region_es_east_1() {
        let config: Config = toml::from_str(
            r#"
        [inner]
        region = "us-east-1"
        "#,
        )
        .unwrap();

        let region: Region = config.inner.region.try_into().unwrap();
        assert_eq!(region, Region::UsEast1);
    }

    #[test]
    fn custom_name_endpoint_localhost() {
        let config: Config = toml::from_str(
            r#"
        [inner]
        endpoint = "http://localhost:9000"
        "#,
        )
        .unwrap();

        let expected_region = Region::Custom {
            name: "custom".into(),
            endpoint: "http://localhost:9000".into(),
        };

        let region: Region = config.inner.region.try_into().unwrap();
        assert_eq!(region, expected_region);
    }

    #[test]
    fn region_not_provided() {
        let config: Config = toml::from_str(
            r#"
        [inner]
        endpoint_is_spelled_wrong = "http://localhost:9000"
        "#,
        )
        .unwrap();

        let region: Result<Region, ParseError> = config.inner.region.try_into();
        match region {
            Err(ParseError::MissingRegionAndEndpoint) => {}
            other => panic!("assertion failed, wrong result {:?}", other),
        }
    }

    #[test]
    fn region_from_endpoint_localhost() {
        assert_eq!(
            region_from_endpoint("http://localhost:9000").unwrap(),
            Region::Custom {
                name: "custom".into(),
                endpoint: "http://localhost:9000".into()
            }
        );
    }

    #[test]
    fn region_from_endpoint_standard_region() {
        assert_eq!(
            region_from_endpoint(
                "https://this-is-a-test-5dec2c2qbgsuekvsecuylqu.us-west-2.es.amazonaws.com"
            )
            .unwrap(),
            Region::Custom {
                name: "us-west-2".into(),
                endpoint:
                    "https://this-is-a-test-5dec2c2qbgsuekvsecuylqu.us-west-2.es.amazonaws.com"
                        .into()
            }
        );
    }

    #[test]
    fn region_from_endpoint_strips_path_query() {
        assert_eq!(
            region_from_endpoint("http://localhost:9000/path?query").unwrap(),
            Region::Custom {
                name: "custom".into(),
                endpoint: "http://localhost:9000".into()
            }
        );
    }
}
