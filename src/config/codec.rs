use serde::{
    de::{self, IntoDeserializer, MapAccess, Visitor},
    Deserialize, Deserializer, Serialize,
};
use std::fmt::{self, Debug};
use vector_core::{
    event::Event,
    transform::{FunctionTransform, Transform},
};

#[derive(Serialize, Debug, Eq, PartialEq, Clone)]
pub struct CodecsConfig {
    pub codec: Vec<CodecConfig>,
}

impl<'de> Deserialize<'de> for CodecsConfig {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct CodecsConfig {
            codec: CodecsConfigValue,
        }

        #[derive(Deserialize)]
        #[serde(untagged)]
        enum CodecsConfigValue {
            Single(CodecConfig),
            Multiple(Vec<CodecConfig>),
        }

        let config = CodecsConfig::deserialize(deserializer)?;

        Ok(Self {
            codec: match config.codec {
                CodecsConfigValue::Single(config) => vec![config],
                CodecsConfigValue::Multiple(configs) => configs,
            },
        })
    }
}

impl From<Vec<CodecConfig>> for CodecsConfig {
    fn from(configs: Vec<CodecConfig>) -> Self {
        Self { codec: configs }
    }
}

impl IntoIterator for CodecsConfig {
    type Item = CodecConfig;
    type IntoIter = std::vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        self.codec.into_iter()
    }
}

#[derive(Serialize, Debug, Eq, PartialEq, Clone)]
pub struct CodecConfig(pub(crate) Codec);

impl CodecConfig {
    pub fn build(&self) -> Transform {
        #[derive(Copy, Clone)]
        struct NoopTransform;

        impl FunctionTransform for NoopTransform {
            fn transform(&mut self, output: &mut Vec<Event>, event: Event) {
                output.push(event)
            }
        }

        Transform::function(NoopTransform)
    }
}

impl<'de> Deserialize<'de> for CodecConfig {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct StringOrStruct;

        impl<'de> Visitor<'de> for StringOrStruct {
            type Value = CodecConfig;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("string or map")
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(CodecConfig(Codec::deserialize(value.into_deserializer())?))
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
pub enum Codec {
    Utf8,
    Json,
}

#[cfg(test)]
mod tests {
    use super::*;
    use indoc::indoc;

    #[test]
    fn config_codecs_single() {
        let config: CodecsConfig = toml::from_str(indoc! {r#"
            codec = "json"
        "#})
        .unwrap();

        assert_eq!(config, vec![CodecConfig(Codec::Json)].into());
    }

    #[test]
    fn config_codecs_multiple() {
        let config: CodecsConfig = toml::from_str(indoc! {r#"
            codec = ["utf8", "json"]
        "#})
        .unwrap();

        assert_eq!(
            config,
            vec![CodecConfig(Codec::Utf8), CodecConfig(Codec::Json)].into()
        );
    }
}
