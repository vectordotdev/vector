use chrono::{DateTime, Local, ParseError, TimeZone as _, Utc};
use chrono_tz::Tz;
use std::fmt::Debug;

#[derive(Clone, Copy, Debug)]
pub enum TimeZone {
    Local,
    Named(Tz),
}

/// This is a wrapper trait to allow `TimeZone` types to be passed genericly.
impl TimeZone {
    pub(super) fn datetime_from_str(
        &self,
        s: &str,
        format: &str,
    ) -> Result<DateTime<Utc>, ParseError> {
        match self {
            Self::Local => Local.datetime_from_str(s, format).map(datetime_to_utc),
            Self::Named(tz) => tz.datetime_from_str(s, format).map(datetime_to_utc),
        }
    }
}

/// Convert a timestamp with a non-UTC time zone into UTC
pub(super) fn datetime_to_utc<TZ: chrono::TimeZone>(ts: DateTime<TZ>) -> DateTime<Utc> {
    Utc.timestamp(ts.timestamp(), ts.timestamp_subsec_nanos())
}
