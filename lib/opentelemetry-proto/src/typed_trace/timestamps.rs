//! OTLP `fixed64` nanosecond timestamp conversion.

use std::time::Duration;

use chrono::{DateTime, Utc};
use vector_core::event::typed_trace::{TraceConversionIssue, TraceConversionReporter};

const NANOS_PER_SECOND: u64 = 1_000_000_000;

pub fn otlp_nanos_to_datetime(nanos: u64) -> DateTime<Utc> {
    // `u64::MAX` nanoseconds falls in the year 2554, well inside chrono's range.
    let seconds = i64::try_from(nanos / NANOS_PER_SECOND).unwrap_or(i64::MAX);
    let subsec = u32::try_from(nanos % NANOS_PER_SECOND).unwrap_or_default();
    DateTime::from_timestamp(seconds, subsec).unwrap_or(DateTime::<Utc>::MAX_UTC)
}

pub fn datetime_to_otlp_nanos(
    ts: DateTime<Utc>,
    reporter: &mut impl TraceConversionReporter,
) -> u64 {
    let nanos = i128::from(ts.timestamp()) * i128::from(NANOS_PER_SECOND)
        + i128::from(ts.timestamp_subsec_nanos());
    u64::try_from(nanos).unwrap_or_else(|_| {
        reporter.report(TraceConversionIssue::ClampedTimestamp);
        if nanos < 0 { 0 } else { u64::MAX }
    })
}

pub fn duration_from_otlp(
    start: u64,
    end: u64,
    reporter: &mut impl TraceConversionReporter,
) -> Duration {
    end.checked_sub(start).map_or_else(
        || {
            reporter.report(TraceConversionIssue::ReversedSpanTimestamps);
            Duration::ZERO
        },
        Duration::from_nanos,
    )
}

pub fn reconstruct_end_time(
    start_nanos: u64,
    duration: Duration,
    reporter: &mut impl TraceConversionReporter,
) -> u64 {
    let end = u128::from(start_nanos) + duration.as_nanos();
    u64::try_from(end).unwrap_or_else(|_| {
        reporter.report(TraceConversionIssue::ClampedTimestamp);
        u64::MAX
    })
}
