use std::{convert::TryFrom, str::FromStr};

use http::{uri::InvalidUri, Uri};
use rusoto_core::{region::ParseRegionError, Region};
use snafu::{ResultExt, Snafu};

pub use crate::aws::region::RegionOrEndpoint;

#[derive(Debug, Snafu)]
pub enum ParseError {
    #[snafu(display("Failed to parse custom endpoint as URI: {}", source))]
    EndpointParseError { source: InvalidUri },
    #[snafu(display("Failed to parse region: {}", source))]
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
    let name = uri
        .host()
        .and_then(region_name_from_host)
        .unwrap_or_else(|| Region::default().name().into());
    let endpoint = strip_endpoint(&uri);
    Ok(Region::Custom { name, endpoint })
}

/// Reconstitute the endpoint from the URI, but strip off all path components
fn strip_endpoint(uri: &Uri) -> String {
    let pq_len = uri
        .path_and_query()
        .map(|pq| pq.as_str().len())
        .unwrap_or(0);
    let endpoint = uri.to_string();
    endpoint[..endpoint.len() - pq_len].to_string()
}

/// Translate a hostname into a region name by finding the first part of
/// the domain name that matches a known region.
fn region_name_from_host(host: &str) -> Option<String> {
    host.split('.')
        .filter_map(|part| Region::from_str(part).ok())
        .map(|region| region.name().into())
        .next()
}

#[cfg(test)]
mod tests {
    use std::convert::TryInto;

    use indoc::indoc;
    use rusoto_core::Region;
    use serde::Deserialize;

    use super::*;

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
        let config: Config = toml::from_str(indoc! {r#"
            [inner]
            region = "us-east-1"
        "#})
        .unwrap();

        let region: Region = config.inner.region.try_into().unwrap();
        assert_eq!(region, Region::UsEast1);
    }

    #[test]
    fn custom_name_endpoint_localhost() {
        let config: Config = toml::from_str(indoc! {r#"
            [inner]
            endpoint = "http://localhost:9000"
        "#})
        .unwrap();

        let expected_region = Region::Custom {
            name: "us-east-1".into(),
            endpoint: "http://localhost:9000".into(),
        };

        let region: Region = config.inner.region.try_into().unwrap();
        assert_eq!(region, expected_region);
    }

    #[test]
    fn region_not_provided() {
        let config: Config = toml::from_str(indoc! {r#"
            [inner]
            endpoint_is_spelled_wrong = "http://localhost:9000"
        "#})
        .unwrap();

        let region: Result<Region, ParseError> = config.inner.region.try_into();
        match region {
            Err(ParseError::MissingRegionAndEndpoint) => {}
            other => panic!("Assertion failed, wrong result {:?}", other),
        }
    }

    #[test]
    fn extracts_region_name_from_host() {
        assert_eq!(region_name_from_host("localhost"), None);
        assert_eq!(
            region_name_from_host("us-west-1.es.amazonaws.com"),
            Some("us-west-1".into())
        );
        assert_eq!(
            region_name_from_host("this-is-a-test.us-west-2.es.amazonaws.com"),
            Some("us-west-2".into())
        );
        assert_eq!(
            region_name_from_host("test.cn-north-1.es.amazonaws.com.cn"),
            Some("cn-north-1".into())
        );
    }

    #[test]
    fn region_from_endpoint_localhost() {
        assert_eq!(
            region_from_endpoint("http://localhost:9000").unwrap(),
            Region::Custom {
                name: "us-east-1".into(),
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
    fn region_from_endpoint_without_scheme() {
        assert_eq!(
            region_from_endpoint("ams3.digitaloceanspaces.com").unwrap(),
            Region::Custom {
                name: "us-east-1".into(),
                endpoint: "ams3.digitaloceanspaces.com".into()
            }
        );
        assert_eq!(
            region_from_endpoint("https://ams3.digitaloceanspaces.com/").unwrap(),
            Region::Custom {
                name: "us-east-1".into(),
                endpoint: "https://ams3.digitaloceanspaces.com".into()
            }
        );
    }

    #[test]
    fn region_from_endpoint_strips_path_query() {
        assert_eq!(
            region_from_endpoint("http://localhost:9000/path?query").unwrap(),
            Region::Custom {
                name: "us-east-1".into(),
                endpoint: "http://localhost:9000".into()
            }
        );
    }
}
