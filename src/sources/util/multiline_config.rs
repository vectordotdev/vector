use std::{convert::TryFrom, time::Duration};

use regex::bytes::Regex;
use snafu::{ResultExt, Snafu};
use vector_config::configurable_component;

use crate::line_agg;

/// Configuration of multi-line aggregation.
#[configurable_component]
#[derive(Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct MultilineConfig {
    /// Regular expression pattern that is used to match the start of a new message.
    pub start_pattern: String,

    /// Regular expression pattern that is used to determine whether or not more lines should be read.
    ///
    /// This setting must be configured in conjunction with `mode`.
    pub condition_pattern: String,

    /// Aggregation mode.
    ///
    /// This setting must be configured in conjunction with `condition_pattern`.
    pub mode: line_agg::Mode,

    /// The maximum amount of time to wait for the next additional line, in milliseconds.
    ///
    /// Once this timeout is reached, the buffered message is guaranteed to be flushed, even if incomplete.
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
            .with_context(|_| InvalidMultilineStartPatternSnafu { start_pattern })?;
        let condition_pattern = Regex::new(condition_pattern)
            .with_context(|_| InvalidMultilineConditionPatternSnafu { condition_pattern })?;
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
