use crate::codecs::{CodecHint, CodecTransform};
use lazy_static::lazy_static;
use serde::{Deserialize, Deserializer, Serialize};
use std::collections::HashMap;

lazy_static! {
    static ref CODECS: HashMap<&'static str, &'static dyn crate::codecs::Codec> =
        inventory::iter::<Box<dyn crate::codecs::Codec>>
            .into_iter()
            .map(|codec| (codec.name(), &**codec))
            .collect();
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct CodecsConfig(pub Box<[CodecConfig]>);

impl<'de> Deserialize<'de> for CodecsConfig {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum CodecsConfig {
            Single(CodecConfig),
            Multiple(Vec<CodecConfig>),
        }

        let config = CodecsConfig::deserialize(deserializer)?;

        Ok(Self(
            match config {
                CodecsConfig::Single(config) => vec![config],
                CodecsConfig::Multiple(configs) => configs,
            }
            .into(),
        ))
    }
}

impl IntoIterator for CodecsConfig {
    type Item = CodecConfig;
    type IntoIter = std::vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        Into::<Vec<_>>::into(self.0).into_iter()
    }
}

impl From<Vec<CodecConfig>> for CodecsConfig {
    fn from(configs: Vec<CodecConfig>) -> Self {
        Self(configs.into())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum CodecConfig {
    String(String),
    Object(Box<dyn crate::codecs::Codec>),
}

impl CodecConfig {
    pub fn name(&self) -> String {
        match &self {
            Self::String(string) => string.into(),
            Self::Object(codec) => codec.name().to_owned(),
        }
    }

    pub fn build(&self, hint: CodecHint) -> crate::Result<CodecTransform> {
        match &self {
            Self::String(string) => match CODECS.get(string.as_str()) {
                Some(codec) => codec.build(hint),
                _ => Err(format!(r#"Unknown codec "{}""#, string).into()),
            },
            Self::Object(codec) => codec.build(hint),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use indoc::indoc;

    #[derive(Debug, Deserialize)]
    struct Config {
        pub codec: CodecsConfig,
    }

    #[test]
    fn config_codecs_single() {
        let config: Config = toml::from_str(indoc! {r#"
            codec = "noop"
        "#})
        .unwrap();
        let codecs = config.codec.0;

        assert_eq!(codecs.len(), 1);
        assert_eq!(codecs[0].name(), "noop");
    }

    #[test]
    fn config_codecs_multiple() {
        let config: Config = toml::from_str(indoc! {r#"
            codec = ["noop", "noop"]
        "#})
        .unwrap();
        let codecs = config.codec.0;

        assert_eq!(codecs.len(), 2);
        assert_eq!(codecs[0].name(), "noop");
        assert_eq!(codecs[1].name(), "noop");
    }

    #[test]
    fn config_codecs_with_options() {
        let config: Config = toml::from_str(indoc! {r#"
            [codec]
            type = "noop"
        "#})
        .unwrap();
        let codecs = config.codec.0;

        assert_eq!(codecs.len(), 1);
        assert_eq!(codecs[0].name(), "noop");
    }

    #[test]
    fn config_codecs_with_options_multiple() {
        let config: Config = toml::from_str(indoc! {r#"
            [[codec]]
            type = "noop"

            [[codec]]
            type = "noop"
        "#})
        .unwrap();
        let codecs = config.codec.0;

        assert_eq!(codecs.len(), 2);
        assert_eq!(codecs[0].name(), "noop");
        assert_eq!(codecs[1].name(), "noop");
    }

    #[test]
    fn build_codec() {
        let config: Config = toml::from_str(indoc! {r#"
            codec = "noop"
        "#})
        .unwrap();
        let codecs = config.codec.0;
        let decoder = codecs[0].build(CodecHint::Decoder);
        let encoder = codecs[0].build(CodecHint::Encoder);

        assert!(decoder.is_ok());
        assert!(encoder.is_ok());
    }

    #[test]
    fn build_codec_unknown() {
        let config: Config = toml::from_str(indoc! {r#"
            codec = "unknown"
        "#})
        .unwrap();
        let codecs = config.codec.0;
        let decoder = codecs[0].build(CodecHint::Decoder);
        let encoder = codecs[0].build(CodecHint::Encoder);

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
