use lazy_static::lazy_static;
use serde::{Deserialize, Deserializer, Serialize};
use std::collections::HashMap;
use vector_core::transform::Transform;

lazy_static! {
    static ref CODECS: HashMap<&'static str, &'static dyn crate::codecs::Codec> =
        inventory::iter::<&dyn crate::codecs::Codec>
            .into_iter()
            .map(|codec| (codec.name(), *codec))
            .collect();
}

#[derive(Debug, Serialize, Default)]
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

#[derive(Debug, Serialize, Deserialize)]
pub struct CodecConfig(pub(crate) Codec);

impl CodecConfig {
    pub fn name(&self) -> String {
        match &self.0 {
            Codec::String(string) => string.into(),
            Codec::Object(codec) => codec.name().to_owned(),
        }
    }

    pub fn build_decoder(&self) -> crate::Result<Transform> {
        match &self.0 {
            Codec::String(string) => match CODECS.get(string.as_str()) {
                Some(codec) => codec.build_decoder(),
                _ => Err(format!(r#"Unknown codec "{}""#, string).into()),
            },
            Codec::Object(codec) => codec.build_decoder(),
        }
    }

    pub fn build_encoder(&self) -> crate::Result<Transform> {
        match &self.0 {
            Codec::String(string) => match CODECS.get(string.as_str()) {
                Some(codec) => codec.build_encoder(),
                _ => Err(format!(r#"Unknown codec "{}""#, string).into()),
            },
            Codec::Object(codec) => codec.build_encoder(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Codec {
    String(String),
    Object(Box<dyn crate::codecs::Codec>),
}

#[cfg(test)]
mod tests {
    use super::*;
    use indoc::indoc;

    #[test]
    fn config_codecs_single() {
        let config: CodecsConfig = toml::from_str(indoc! {r#"
            codec = "noop"
        "#})
        .unwrap();
        let codecs = config.codec;

        assert_eq!(codecs.len(), 1);
        assert_eq!(codecs[0].name(), "noop");
    }

    #[test]
    fn config_codecs_multiple() {
        let config: CodecsConfig = toml::from_str(indoc! {r#"
            codec = ["noop", "noop"]
        "#})
        .unwrap();
        let codecs = config.codec;

        assert_eq!(codecs.len(), 2);
        assert_eq!(codecs[0].name(), "noop");
        assert_eq!(codecs[1].name(), "noop");
    }

    #[test]
    fn config_codecs_with_options() {
        let config: CodecsConfig = toml::from_str(indoc! {r#"
            [codec]
            type = "noop"
        "#})
        .unwrap();
        let codecs = config.codec;

        assert_eq!(codecs.len(), 1);
        assert_eq!(codecs[0].name(), "noop");
    }

    #[test]
    fn config_codecs_with_options_multiple() {
        let config: CodecsConfig = toml::from_str(indoc! {r#"
            [[codec]]
            type = "noop"

            [[codec]]
            type = "noop"
        "#})
        .unwrap();
        let codecs = config.codec;

        assert_eq!(codecs.len(), 2);
        assert_eq!(codecs[0].name(), "noop");
        assert_eq!(codecs[1].name(), "noop");
    }

    #[test]
    fn build_codec() {
        let config: CodecsConfig = toml::from_str(indoc! {r#"
            codec = "noop"
        "#})
        .unwrap();
        let codecs = config.codec;
        let decoder = codecs[0].build_decoder();
        let encoder = codecs[0].build_encoder();

        assert!(decoder.is_ok());
        assert!(encoder.is_ok());
    }

    #[test]
    fn build_codec_unknown() {
        let config: CodecsConfig = toml::from_str(indoc! {r#"
            codec = "unknown"
        "#})
        .unwrap();
        let codecs = config.codec;
        let decoder = codecs[0].build_decoder();
        let encoder = codecs[0].build_encoder();

        assert_eq!(
            decoder.err().map(|error| error.to_string()),
            Some(r#"Unknown codec "unknown""#.to_owned())
        );
        assert_eq!(
            encoder.err().map(|error| error.to_string()),
            Some(r#"Unknown codec "unknown""#.to_owned())
        );
    }
}
