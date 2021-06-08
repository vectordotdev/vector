use lazy_static::lazy_static;
use serde::{Deserialize, Deserializer, Serialize};
use std::collections::HashMap;
use vector_core::transform::Transform;

lazy_static! {
    static ref FRAMERS: HashMap<&'static str, &'static dyn crate::framers::Framer> =
        inventory::iter::<&dyn crate::framers::Framer>
            .into_iter()
            .map(|framer| (framer.name(), *framer))
            .collect();
}

#[derive(Debug, Serialize, Default)]
pub struct FramingsConfig(pub Vec<FramingConfig>);

impl<'de> Deserialize<'de> for FramingsConfig {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum FramingsConfig {
            Single(FramingConfig),
            Multiple(Vec<FramingConfig>),
        }

        let config = FramingsConfig::deserialize(deserializer)?;

        Ok(Self(match config {
            FramingsConfig::Single(config) => vec![config],
            FramingsConfig::Multiple(configs) => configs,
        }))
    }
}

impl From<Vec<FramingConfig>> for FramingsConfig {
    fn from(configs: Vec<FramingConfig>) -> Self {
        Self(configs)
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum FramingConfig {
    String(String),
    Object(Box<dyn crate::framers::Framer>),
}

impl FramingConfig {
    pub fn name(&self) -> String {
        match &self {
            Self::String(string) => string.into(),
            Self::Object(framer) => framer.name().to_owned(),
        }
    }

    pub fn build(&self) -> crate::Result<Transform<Vec<u8>>> {
        match &self {
            Self::String(string) => match FRAMERS.get(string.as_str()) {
                Some(framer) => framer.build(),
                _ => Err(format!(r#"Unknown framer "{}""#, string).into()),
            },
            Self::Object(framer) => framer.build(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use indoc::indoc;

    #[derive(Debug, Deserialize)]
    struct Config {
        pub framing: FramingsConfig,
    }

    #[test]
    fn config_framers_single() {
        let config: Config = toml::from_str(indoc! {r#"
            framing = "noop"
        "#})
        .unwrap();
        let framers = config.framing.0;

        assert_eq!(framers.len(), 1);
        assert_eq!(framers[0].name(), "noop");
    }

    #[test]
    fn config_framers_multiple() {
        let config: Config = toml::from_str(indoc! {r#"
            framing = ["noop", "noop"]
        "#})
        .unwrap();
        let framers = config.framing.0;

        assert_eq!(framers.len(), 2);
        assert_eq!(framers[0].name(), "noop");
        assert_eq!(framers[1].name(), "noop");
    }

    #[test]
    fn config_framers_with_options() {
        let config: Config = toml::from_str(indoc! {r#"
            [framing]
            type = "noop"
        "#})
        .unwrap();
        let framers = config.framing.0;

        assert_eq!(framers.len(), 1);
        assert_eq!(framers[0].name(), "noop");
    }

    #[test]
    fn config_framers_with_options_multiple() {
        let config: Config = toml::from_str(indoc! {r#"
            [[framing]]
            type = "noop"

            [[framing]]
            type = "noop"
        "#})
        .unwrap();
        let framers = config.framing.0;

        assert_eq!(framers.len(), 2);
        assert_eq!(framers[0].name(), "noop");
        assert_eq!(framers[1].name(), "noop");
    }

    #[test]
    fn build_framer() {
        let config: Config = toml::from_str(indoc! {r#"
            framing = "noop"
        "#})
        .unwrap();
        let framers = config.framing.0;
        let framer = framers[0].build();

        assert!(framer.is_ok());
    }

    #[test]
    fn build_framer_unknown() {
        let config: Config = toml::from_str(indoc! {r#"
            framing = "unknown"
        "#})
        .unwrap();
        let framers = config.framing.0;
        let framer = framers[0].build();

        assert_eq!(
            framer.err().map(|error| error.to_string()),
            Some(r#"Unknown framer "unknown""#.to_owned())
        );
    }
}
