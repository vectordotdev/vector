use std::fmt::Debug;

use chrono::{DateTime, Local, ParseError, TimeZone as _, Utc};
use chrono_tz::Tz;
use derivative::Derivative;

/// Timezone reference.
///
/// This can refer to any valid timezone as defined in the [TZ database][tzdb], or "local" which
/// refers to the system local timezone.
///
/// [tzdb]: https://en.wikipedia.org/wiki/List_of_tz_database_time_zones
#[cfg_attr(
    feature = "serde",
    derive(::serde::Deserialize, ::serde::Serialize),
    serde(try_from = "String", into = "String")
)]
#[derive(Clone, Copy, Debug, Derivative, Eq, PartialEq)]
#[derivative(Default)]
pub enum TimeZone {
    /// System local timezone.
    #[derivative(Default)]
    Local,

    /// A named timezone.
    ///
    /// Must be a valid name in the [TZ database][tzdb].
    ///
    /// [tzdb]: https://en.wikipedia.org/wiki/List_of_tz_database_time_zones
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
    Utc.timestamp_opt(ts.timestamp(), ts.timestamp_subsec_nanos())
        .single()
        .expect("invalid timestamp")
}

impl From<TimeZone> for String {
    fn from(tz: TimeZone) -> Self {
        match tz {
            TimeZone::Local => "local".to_string(),
            TimeZone::Named(tz) => tz.name().to_string(),
        }
    }
}

impl TryFrom<String> for TimeZone {
    type Error = String;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        match TimeZone::parse(&value) {
            Some(tz) => Ok(tz),
            None => Err("No such time zone".to_string()),
        }
    }
}
