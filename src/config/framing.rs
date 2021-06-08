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
pub struct FramingsConfig {
    pub configs: Vec<FramingConfig>,
}

impl<'de> Deserialize<'de> for FramingsConfig {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct FramingsConfig {
            framer: FramingsConfigValue,
        }

        #[derive(Deserialize)]
        #[serde(untagged)]
        enum FramingsConfigValue {
            Single(FramingConfig),
            Multiple(Vec<FramingConfig>),
        }

        let config = FramingsConfig::deserialize(deserializer)?;

        Ok(Self {
            configs: match config.framer {
                FramingsConfigValue::Single(config) => vec![config],
                FramingsConfigValue::Multiple(configs) => configs,
            },
        })
    }
}

impl From<Vec<FramingConfig>> for FramingsConfig {
    fn from(configs: Vec<FramingConfig>) -> Self {
        Self { configs }
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

    #[test]
    fn config_framers_single() {
        let config: FramingsConfig = toml::from_str(indoc! {r#"
            framer = "noop"
        "#})
        .unwrap();
        let framers = config.configs;

        assert_eq!(framers.len(), 1);
        assert_eq!(framers[0].name(), "noop");
    }

    #[test]
    fn config_framers_multiple() {
        let config: FramingsConfig = toml::from_str(indoc! {r#"
            framer = ["noop", "noop"]
        "#})
        .unwrap();
        let framers = config.configs;

        assert_eq!(framers.len(), 2);
        assert_eq!(framers[0].name(), "noop");
        assert_eq!(framers[1].name(), "noop");
    }

    #[test]
    fn config_framers_with_options() {
        let config: FramingsConfig = toml::from_str(indoc! {r#"
            [framer]
            type = "noop"
        "#})
        .unwrap();
        let framers = config.configs;

        assert_eq!(framers.len(), 1);
        assert_eq!(framers[0].name(), "noop");
    }

    #[test]
    fn config_framers_with_options_multiple() {
        let config: FramingsConfig = toml::from_str(indoc! {r#"
            [[framer]]
            type = "noop"

            [[framer]]
            type = "noop"
        "#})
        .unwrap();
        let framers = config.configs;

        assert_eq!(framers.len(), 2);
        assert_eq!(framers[0].name(), "noop");
        assert_eq!(framers[1].name(), "noop");
    }

    #[test]
    fn build_framer() {
        let config: FramingsConfig = toml::from_str(indoc! {r#"
            framer = "noop"
        "#})
        .unwrap();
        let framers = config.configs;
        let framer = framers[0].build();

        assert!(framer.is_ok());
    }

    #[test]
    fn build_framer_unknown() {
        let config: FramingsConfig = toml::from_str(indoc! {r#"
            framer = "unknown"
        "#})
        .unwrap();
        let framers = config.configs;
        let framer = framers[0].build();

        assert_eq!(
            framer.err().map(|error| error.to_string()),
            Some(r#"Unknown framer "unknown""#.to_owned())
        );
    }
}
