use http::Uri;
use rusoto_core::Region;
use serde::{Deserialize, Serialize};
use std::convert::TryFrom;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RegionOrEndpoint {
    Region {
        region: String,
    },
    Endpoint {
        endpoint: String,
        endpoint_name: Option<String>,
    },
}

impl TryFrom<RegionOrEndpoint> for Region {
    type Error = String;

    fn try_from(r: RegionOrEndpoint) -> Result<Self, Self::Error> {
        match r {
            RegionOrEndpoint::Region { region } => region
                .parse()
                .map_err(|e| format!("Region Parse Error: {}", e)),
            RegionOrEndpoint::Endpoint {
                endpoint,
                endpoint_name,
            } => match endpoint.parse::<Uri>() {
                Ok(_) => Ok(Region::Custom {
                    name: endpoint_name.unwrap_or_else(|| "custom".into()),
                    endpoint: endpoint,
                }),
                Err(e) => Err(format!("Custom Endpoint Parse Error: {}", e)),
            },
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
        #[serde(flatten)]
        region: RegionOrEndpoint,
    }

    #[test]
    fn region_es_east_1() {
        let config: Config = toml::from_str(
            r#"
        region = "us-east-1"
        "#,
        )
        .unwrap();

        let region: Region = config.region.try_into().unwrap();
        assert_eq!(region, Region::UsEast1);
    }

    #[test]
    fn region_custom_name_endpoint() {
        let config: Config = toml::from_str(
            r#"
        endpoint = "http://localhost:9000"
        "#,
        )
        .unwrap();

        let expected_region = Region::Custom {
            name: "custom".into(),
            endpoint: "http://localhost:9000".into(),
        };

        let region: Region = config.region.try_into().unwrap();
        assert_eq!(region, expected_region);
    }
}
