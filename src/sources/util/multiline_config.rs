use std::{convert::TryFrom, time::Duration};

use regex::bytes::Regex;
use serde::{Deserialize, Serialize};
use snafu::{ResultExt, Snafu};

use crate::line_agg;

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MultilineConfig {
    pub start_pattern: String,
    pub condition_pattern: String,
    pub mode: line_agg::Mode,
    pub timeout_ms: u64,
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
        let timeout = Duration::from_millis(*timeout_ms);

        Ok(Self {
            start_pattern,
            condition_pattern,
            mode: *mode,
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
