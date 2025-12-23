use arrow::{
    array::{
        ArrayRef, TimestampMicrosecondBuilder, TimestampMillisecondBuilder,
        TimestampNanosecondBuilder, TimestampSecondBuilder,
    },
    datatypes::TimeUnit,
};
use chrono::{DateTime, Utc};
use std::sync::Arc;
use vector_core::event::{Event, Value};

use crate::encoding::format::arrow::ArrowEncodingError;

pub(crate) fn extract_timestamp(value: &Value) -> Option<DateTime<Utc>> {
    match value {
        Value::Timestamp(ts) => Some(*ts),
        Value::Bytes(bytes) => std::str::from_utf8(bytes)
            .ok()
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&Utc)),
        _ => None,
    }
}

pub(crate) fn build_timestamp_array(
    events: &[Event],
    field_name: &str,
    time_unit: TimeUnit,
    nullable: bool,
) -> Result<ArrayRef, ArrowEncodingError> {
    macro_rules! build_array {
        ($builder:ty, $converter:expr) => {{
            let mut builder = <$builder>::with_capacity(events.len());
            for event in events {
                if let Event::Log(log) = event {
                    let value_to_append = log.get(field_name).and_then(|value| {
                        // First, try to extract it as a native or string timestamp
                        if let Some(ts) = extract_timestamp(value) {
                            $converter(&ts)
                        }
                        // Else, fall back to a raw integer
                        else if let Value::Integer(i) = value {
                            Some(*i)
                        }
                        // Else, it's an unsupported type (e.g., Bool, Float)
                        else {
                            None
                        }
                    });

                    if value_to_append.is_none() && !nullable {
                        return Err(ArrowEncodingError::NullConstraint {
                            field_name: field_name.into(),
                        });
                    }

                    builder.append_option(value_to_append);
                }
            }
            Ok(Arc::new(builder.finish()))
        }};
    }

    match time_unit {
        TimeUnit::Second => {
            build_array!(TimestampSecondBuilder, |ts: &DateTime<Utc>| Some(
                ts.timestamp()
            ))
        }
        TimeUnit::Millisecond => {
            build_array!(TimestampMillisecondBuilder, |ts: &DateTime<Utc>| Some(
                ts.timestamp_millis()
            ))
        }
        TimeUnit::Microsecond => {
            build_array!(TimestampMicrosecondBuilder, |ts: &DateTime<Utc>| Some(
                ts.timestamp_micros()
            ))
        }
        TimeUnit::Nanosecond => {
            build_array!(TimestampNanosecondBuilder, |ts: &DateTime<Utc>| ts
                .timestamp_nanos_opt())
        }
    }
}
