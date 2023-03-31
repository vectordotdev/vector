use chrono::{DateTime, FixedOffset, ParseError};
use std::{
    convert::{TryFrom, Into},
    default::Default,
};

use vector_config::configurable_component;


/// handle tz offset configuration
#[configurable_component]
#[derive(Clone, Debug, Eq, PartialEq)]
#[serde(try_from = "String", into = "String")]
pub struct TzOffset(FixedOffset);

impl TzOffset {
    pub fn offset(&self) -> FixedOffset {
        self.0
    }
}

impl TryFrom<String> for TzOffset {
    type Error = ParseError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        let dt = DateTime::parse_from_str(
            format!("2000-01-01 00:00:00 {}", value).as_str(),
            "%Y-%m-%d %H:%M:%S %z"
        )?;
        Ok(TzOffset(*dt.offset()))
    }
}

impl TryFrom<&str> for TzOffset {
    type Error = ParseError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        let dt = DateTime::parse_from_str(
            format!("2000-01-01 00:00:00 {}", value).as_str(),
            "%Y-%m-%d %H:%M:%S %z"
        )?;
        Ok(TzOffset(*dt.offset()))
    }
}

impl Into<String> for TzOffset {
    fn into(self) -> String {
        self.0.to_string()
    }
}

impl Default for TzOffset {
    fn default() -> TzOffset {
        TzOffset(FixedOffset::east_opt(0).unwrap())
    }
}

impl std::fmt::Display for TzOffset {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use chrono::FixedOffset;
    use super::*;

    #[test]
    fn parse_str() {
        let tz_offset = TzOffset::try_from("+08:00").unwrap();

        assert_eq!(
            tz_offset.to_string(),
            "+08:00".to_string()
        );
    }

    #[test]
    fn parse_string() {
        let tz_offset = TzOffset::try_from("+08:00".to_string()).unwrap();

        assert_eq!(
            tz_offset.to_string(),
            "+08:00".to_string()
        );
    }

    #[test]
    fn check_offset() {
        let fixed_offset = FixedOffset::east_opt(28800).unwrap();
        let tz_offset = TzOffset::try_from("+08:00".to_string()).unwrap();
        assert_eq!(
            tz_offset.offset(),
            fixed_offset
        );
    }
}
