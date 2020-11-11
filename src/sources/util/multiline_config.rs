use crate::line_agg;

use regex::bytes::Regex;
use serde::{Deserialize, Serialize};
use snafu::{ResultExt, Snafu};
use std::convert::TryFrom;
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MultilineConfig {
    pub start_pattern: String,
    pub condition_pattern: String,
    pub mode: line_agg::Mode,
    #[serde(default = "default_timeout_ms", with = "optional_u64")]
    pub timeout_ms: Option<u64>,
}

const fn default_timeout_ms() -> Option<u64> {
    Some(1000)
}

impl TryFrom<&MultilineConfig> for line_agg::Config {
    type Error = Error;

    fn try_from(config: &MultilineConfig) -> Result<Self, Self::Error> {
        let MultilineConfig {
            start_pattern,
            condition_pattern,
            mode,
            timeout_ms,
        } = config;

        let start_pattern = Regex::new(start_pattern)
            .with_context(|| InvalidMultilineStartPattern { start_pattern })?;
        let condition_pattern = Regex::new(condition_pattern)
            .with_context(|| InvalidMultilineConditionPattern { condition_pattern })?;
        let mode = mode.clone();
        let timeout = timeout_ms.map(Duration::from_millis);

        Ok(Self {
            start_pattern,
            condition_pattern,
            mode,
            timeout,
        })
    }
}

#[derive(Debug, Snafu)]
pub enum Error {
    #[snafu(display(
        "unable to parse multiline start pattern from {:?}: {}",
        start_pattern,
        source
    ))]
    InvalidMultilineStartPattern {
        start_pattern: String,
        source: regex::Error,
    },
    #[snafu(display(
        "unable to parse multiline condition pattern from {:?}: {}",
        condition_pattern,
        source
    ))]
    InvalidMultilineConditionPattern {
        condition_pattern: String,
        source: regex::Error,
    },
}

mod optional_u64 {
    use serde::de::{self, Deserializer, Unexpected, Visitor};
    use serde::ser::Serializer;

    pub(super) fn deserialize<'de, D>(deserializer: D) -> Result<Option<u64>, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct OptionalU64;

        impl<'de> Visitor<'de> for OptionalU64 {
            type Value = Option<u64>;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("number or \"none\"")
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                match value {
                    "none" => Ok(None),
                    s => Err(de::Error::invalid_value(
                        Unexpected::Str(s),
                        &"number or \"none\"",
                    )),
                }
            }

            fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                if value > 0 {
                    Ok(Some(value as u64))
                } else {
                    Err(de::Error::invalid_value(
                        Unexpected::Signed(value),
                        &"positive integer",
                    ))
                }
            }

            fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(Some(value))
            }
        }

        deserializer.deserialize_any(OptionalU64)
    }

    pub(super) fn serialize<S>(value: &Option<u64>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match *value {
            Some(v) => serializer.serialize_u64(v),
            None => serializer.serialize_str("none"),
        }
    }
}
