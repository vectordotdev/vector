use http::Uri;
use rusoto_core::Region;
use serde::{Deserialize, Serialize};
use std::convert::TryFrom;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct RegionOrEndpoint {
    region: Option<String>,
    endpoint: Option<String>,
}

impl TryFrom<RegionOrEndpoint> for Region {
    type Error = String;

    fn try_from(r: RegionOrEndpoint) -> Result<Self, Self::Error> {
        if let Some(region) = r.region {
            let region = region.parse().map_err(|e| format!("{}", e))?;

            if !r.endpoint.is_some() {
                Ok(region)
            } else {
                Err("Only one of 'region' or 'endpoint' can be specified".into())
            }
        } else if let Some(endpoint) = r.endpoint {
            endpoint
                .parse::<Uri>()
                .map(|_| Region::Custom {
                    name: "custom".into(),
                    endpoint,
                })
                .map_err(|e| format!("Custom Endpoint Parse Error: {}", e))
        } else {
            Err(format!("Must set 'region' or 'endpoint'"))
        }
    }
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
    fn region_custom_name_endpoint() {
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

        let region: Result<Region, String> = config.inner.region.try_into();
        assert!(region.is_err());
    }
}
