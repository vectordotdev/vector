use chrono::ParseError;
use chrono::TimeZone as ChronoTimeZone;
use chrono::{DateTime, Local, Utc};
use chrono_tz::Tz;

#[derive(Default)]
pub enum TimeZone {
    /// System local timezone.
    #[default]
    Local,

    /// A named timezone.
    ///
    /// Must be a valid name in the [TZ database][tzdb].
    ///
    /// [tzdb]: https://en.wikipedia.org/wiki/List_of_tz_database_time_zones
    Named(Tz),
}

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
pub fn datetime_to_utc<TZ: chrono::TimeZone>(ts: &DateTime<TZ>) -> DateTime<Utc> {
    Utc.timestamp_opt(ts.timestamp(), ts.timestamp_subsec_nanos())
        .single()
        .expect("invalid timestamp")
}
