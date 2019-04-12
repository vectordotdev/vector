use http::Uri;
use rusoto_core::Region;
use serde::{
    de::{Deserialize, Deserializer, Error, MapAccess, Visitor},
    ser::{Serialize, Serializer},
};
use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub enum RegionOrEndpoint {
    Region(Region),
    Endpoint(Uri),
}

impl<'de> Deserialize<'de> for RegionOrEndpoint {
    fn deserialize<D>(d: D) -> Result<RegionOrEndpoint, D::Error>
    where
        D: Deserializer<'de>,
    {
        d.deserialize_map(RegionOrEndpointVisitor)
    }
}

impl Serialize for RegionOrEndpoint {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            RegionOrEndpoint::Region(r) => serializer.serialize_some(r),
            RegionOrEndpoint::Endpoint(e) => {
                let s = format!("{}", e);
                serializer.serialize_str(&s)
            }
        }
    }
}

struct RegionOrEndpointVisitor;

impl<'de> Visitor<'de> for RegionOrEndpointVisitor {
    type Value = RegionOrEndpoint;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "expected a region string or a name/endpoint map")
    }

    fn visit_str<E>(self, s: &str) -> Result<Self::Value, E>
    where
        E: Error,
    {
        match s.parse::<Region>() {
            Ok(r) => Ok(RegionOrEndpoint::Region(r)),
            Err(_) => Err(Error::custom("Not a valid region, please use one of the provided regions https://docs.aws.amazon.com/general/latest/gr/rande.html")),
        }
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let entry: (&str, &str) = map
            .next_entry()?
            .ok_or_else(|| Error::custom("Expected either `region` or `endpoint`"))?;

        if entry.0 == "region" {
            self.visit_str(&entry.1)
        } else if entry.0 == "endpoint" {
            match entry.1.parse::<Uri>() {
                Ok(uri) => Ok(RegionOrEndpoint::Endpoint(uri)),
                Err(e) => Err(Error::custom(format!("Expected a valid Uri; {}", e))),
            }
        } else {
            Err(Error::custom("Expected either `region` or `endpoint`"))
        }
    }
}

impl From<RegionOrEndpoint> for Region {
    fn from(r: RegionOrEndpoint) -> Self {
        match r {
            RegionOrEndpoint::Region(r) => r,
            RegionOrEndpoint::Endpoint(e) => Region::Custom {
                name: "custom".into(),
                endpoint: e.to_string(),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusoto_core::Region;
    use serde::Deserialize;

    #[derive(Deserialize)]
    struct Config {
        #[serde(deserialize_with = "crate::region::deserialize")]
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

        let region = Region::from(config.region);
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
        let region = Region::from(config.region);
        assert_eq!(region, expected_region);
    }
}
