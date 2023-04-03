//use chrono::{DateTime, FixedOffset, ParseError};
use chrono_tz::{UTC, Tz, ParseError};
use std::{
    convert::{TryFrom, Into},
    default::Default,
};

use vector_config::configurable_component;


/// handle tz offset configuration
#[configurable_component]
#[derive(Clone, Debug, Eq, PartialEq)]
#[serde(try_from = "String", into = "String")]
pub struct PathTz(Tz);

impl PathTz {
    pub fn timezone(&self) -> Tz {
        self.0
    }
}

impl TryFrom<String> for PathTz {
    type Error = ParseError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        let tz: Tz = value.parse()?;
        Ok(PathTz(tz))
    }
}

impl TryFrom<&str> for PathTz {
    type Error = ParseError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        let tz: Tz = value.parse()?;
        Ok(PathTz(tz))
    }
}

impl Into<String> for PathTz {
    fn into(self) -> String {
        self.0.to_string()
    }
}

impl Default for PathTz {
    fn default() -> PathTz {
        PathTz(UTC)
    }
}

// impl std::fmt::Display for TzOffset {
//     fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
//         write!(f, "{}", self.0)
//     }
// }

#[cfg(test)]
mod tests {
    use chrono_tz::{UTC, Tz, Asia::Singapore};
    use super::*;

    #[test]
    fn parse_str() {
        let path_tz = PathTz::try_from("Asia/Singapore").unwrap();
        let tz: Tz = path_tz.timezone();

        assert_eq!(
            tz,
            Singapore
        );
    }

    #[test]
    fn parse_string() {
        let path_tz = PathTz::try_from("Asia/Singapore".to_string()).unwrap();
        let tz: Tz = path_tz.timezone();

        assert_eq!(
            tz,
            Singapore
        );
    }

    #[test]
    fn utc_timezone() {
        let path_tz = PathTz::try_from("UTC").unwrap();
        let tz: Tz = path_tz.timezone();

        assert_eq!(
            tz,
            UTC
        );
    }
}
