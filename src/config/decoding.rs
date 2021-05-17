use crate::codec::decoders::Decoder;
use serde::{
    de::{self, IntoDeserializer, MapAccess, Visitor},
    Deserialize, Deserializer, Serialize,
};
use std::fmt::{self, Debug};
use vector_core::event::Event;

#[derive(Serialize, Debug, Eq, PartialEq, Clone)]
pub struct DecodingsConfig {
    pub decoding: Vec<DecodingConfig>,
}

impl<'de> Deserialize<'de> for DecodingsConfig {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct DecodingsConfig {
            decoding: DecodingsConfigValue,
        }

        #[derive(Deserialize)]
        #[serde(untagged)]
        enum DecodingsConfigValue {
            Single(DecodingConfig),
            Multiple(Vec<DecodingConfig>),
        }

        let config = DecodingsConfig::deserialize(deserializer)?;

        Ok(Self {
            decoding: match config.decoding {
                DecodingsConfigValue::Single(config) => vec![config],
                DecodingsConfigValue::Multiple(configs) => configs,
            },
        })
    }
}

impl From<Vec<DecodingConfig>> for DecodingsConfig {
    fn from(configs: Vec<DecodingConfig>) -> Self {
        Self { decoding: configs }
    }
}

impl IntoIterator for DecodingsConfig {
    type Item = DecodingConfig;
    type IntoIter = std::vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        self.decoding.into_iter()
    }
}

#[derive(Serialize, Debug, Eq, PartialEq, Clone)]
pub struct DecodingConfig(pub(crate) Decoding);

impl DecodingConfig {
    pub fn decode(self, event: Event) -> Event {
        let decoder = Into::<Decoder>::into(self);

        decoder.decode(event)
    }
}

impl<'de> Deserialize<'de> for DecodingConfig {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct StringOrStruct;

        impl<'de> Visitor<'de> for StringOrStruct {
            type Value = DecodingConfig;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("string or map")
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(DecodingConfig(Decoding::deserialize(
                    value.into_deserializer(),
                )?))
            }

            fn visit_map<M>(self, map: M) -> std::result::Result<Self::Value, M::Error>
            where
                M: MapAccess<'de>,
            {
                Deserialize::deserialize(de::value::MapAccessDeserializer::new(map))
            }
        }

        deserializer.deserialize_any(StringOrStruct)
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Decoding {
    Utf8,
    Json,
}

#[cfg(test)]
mod tests {
    use super::*;
    use indoc::indoc;

    #[test]
    fn config_decodings_single() {
        let config: DecodingsConfig = toml::from_str(indoc! {r#"
            decoding = "json"
        "#})
        .unwrap();

        assert_eq!(config, vec![DecodingConfig(Decoding::Json)].into());
    }

    #[test]
    fn config_decodings_multiple() {
        let config: DecodingsConfig = toml::from_str(indoc! {r#"
            decoding = ["utf8", "json"]
        "#})
        .unwrap();

        assert_eq!(
            config,
            vec![
                DecodingConfig(Decoding::Utf8),
                DecodingConfig(Decoding::Json)
            ]
            .into()
        );
    }
}
