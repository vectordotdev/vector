use rusoto_core::Region;
use serde::de::{Deserializer, Error, MapAccess, Visitor};
use std::fmt;

pub fn deserialize<'de, D>(d: D) -> Result<Region, D::Error>
where
    D: Deserializer<'de>,
{
    d.deserialize_any(RegionVisitor)
}

struct RegionVisitor;

impl<'de> Visitor<'de> for RegionVisitor {
    type Value = Region;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "expected a region string or a name/endpoint map")
    }

    fn visit_str<E>(self, s: &str) -> Result<Self::Value, E>
    where
        E: Error,
    {
        match s.parse::<Region>() {
            Ok(r) => Ok(r),
            Err(_) => Err(Error::custom("Not a valid region, please use one of the provided regions https://docs.aws.amazon.com/general/latest/gr/rande.html")),
        }
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let entry1: (String, String) = map
            .next_entry()?
            .ok_or_else(|| Error::custom("Expected either `name` or `endpoint`"))?;
        let entry2: (String, String) = map
            .next_entry()?
            .ok_or_else(|| Error::custom("Expected either `name` or `endpoint`"))?;

        if entry1.0.as_str() == "name" {
            if entry2.0.as_str() == "endpoint" {
                Ok(Region::Custom {
                    name: entry1.1,
                    endpoint: entry2.1,
                })
            } else {
                Err(Error::custom("Expected an `endpoint` key/value"))
            }
        } else if entry1.0.as_str() == "endpoint" {
            if entry2.0.as_str() == "name" {
                Ok(Region::Custom {
                    name: entry2.1,
                    endpoint: entry1.1,
                })
            } else {
                Err(Error::custom("Expected a `name` key/value"))
            }
        } else {
            Err(Error::custom("Expected a `name` and `endpoint` key/value"))
        }
    }
}

#[cfg(test)]
mod tests {
    use rusoto_core::Region;
    use serde::Deserialize;

    #[derive(Deserialize)]
    struct Config {
        #[serde(deserialize_with = "crate::region::deserialize")]
        region: Region,
    }

    #[test]
    fn region_es_east_1() {
        let config: Config = toml::from_str(
            r#"
        region = "us-east-1"
        "#,
        )
        .unwrap();

        assert_eq!(config.region, Region::UsEast1);
    }

    #[test]
    fn region_custom_name_endpoint() {
        let config: Config = toml::from_str(
            r#"
        [region]
        name = "local"
        endpoint = "http://localhost:9000"
        "#,
        )
        .unwrap();

        let expected_region = Region::Custom {
            name: "local".into(),
            endpoint: "http://localhost:9000".into(),
        };
        assert_eq!(config.region, expected_region);
    }

    #[test]
    fn region_custom_endpoint_name() {
        let config: Config = toml::from_str(
            r#"
        [region]
        endpoint = "http://localhost:9000"
        name = "local"
        "#,
        )
        .unwrap();

        let expected_region = Region::Custom {
            name: "local".into(),
            endpoint: "http://localhost:9000".into(),
        };
        assert_eq!(config.region, expected_region);
    }
}
