use std::fmt::Debug;

use chrono::{DateTime, Local, ParseError, TimeZone as _, Utc};
use chrono_tz::Tz;
use derivative::Derivative;

#[derive(Clone, Copy, Debug, Derivative, Eq, PartialEq)]
#[derivative(Default)]
pub enum TimeZone {
    #[derivative(Default)]
    Local,
    Named(Tz),
}

/// This is a wrapper trait to allow `TimeZone` types to be passed generically.
impl TimeZone {
    /// Parse a date/time string into `DateTime<Utc>`.
    ///
    /// # Errors
    ///
    /// Returns parse errors from the underlying time parsing functions.
    pub fn datetime_from_str(&self, s: &str, format: &str) -> Result<DateTime<Utc>, ParseError> {
        match self {
            Self::Local => Local
                .datetime_from_str(s, format)
                .map(|dt| datetime_to_utc(&dt)),
            Self::Named(tz) => tz
                .datetime_from_str(s, format)
                .map(|dt| datetime_to_utc(&dt)),
        }
    }

    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "" | "local" => Some(Self::Local),
            _ => s.parse::<Tz>().ok().map(Self::Named),
        }
    }
}

/// Convert a timestamp with a non-UTC time zone into UTC
pub(super) fn datetime_to_utc<TZ: chrono::TimeZone>(ts: &DateTime<TZ>) -> DateTime<Utc> {
    Utc.timestamp(ts.timestamp(), ts.timestamp_subsec_nanos())
}

#[cfg(feature = "serde")]
pub mod ser_de {
    use serde::{de, Deserialize, Deserializer, Serialize, Serializer};

    use super::TimeZone;

    impl Serialize for TimeZone {
        fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
            match self {
                Self::Local => serializer.serialize_str("local"),
                Self::Named(tz) => serializer.serialize_str(tz.name()),
            }
        }
    }

    impl<'de> Deserialize<'de> for TimeZone {
        fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            deserializer.deserialize_str(TimeZoneVisitor)
        }
    }

    struct TimeZoneVisitor;

    impl<'de> de::Visitor<'de> for TimeZoneVisitor {
        type Value = TimeZone;

        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "a time zone name")
        }

        fn visit_str<E: de::Error>(self, s: &str) -> Result<Self::Value, E> {
            match TimeZone::parse(s) {
                Some(tz) => Ok(tz),
                None => Err(de::Error::custom("No such time zone")),
            }
        }
    }
}
