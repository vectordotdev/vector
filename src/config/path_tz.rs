use chrono_tz::{ParseError, Tz, UTC};
use std::{
    convert::{From, TryFrom},
    default::Default,
};
use vector_config::configurable_component;

/// Configure timezone
#[configurable_component]
#[derive(Clone, Debug, Eq, PartialEq)]
#[serde(try_from = "String", into = "String")]
pub struct PathTz(Tz);

impl PathTz {
    pub const fn timezone(&self) -> Tz {
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

impl From<PathTz> for String {
    fn from(path_tz: PathTz) -> String {
        path_tz.0.to_string()
    }
}

impl Default for PathTz {
    fn default() -> PathTz {
        PathTz(UTC)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono_tz::{Asia::Singapore, Tz, UTC};

    #[test]
    fn parse_string() {
        let path_tz = PathTz::try_from("Asia/Singapore".to_string()).unwrap();
        let tz: Tz = path_tz.timezone();

        assert_eq!(tz, Singapore);
    }

    #[test]
    fn utc_timezone() {
        let path_tz = PathTz::try_from("UTC".to_string()).unwrap();
        let tz: Tz = path_tz.timezone();

        assert_eq!(tz, UTC);
    }
}
