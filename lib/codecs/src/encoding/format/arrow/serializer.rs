//! Arrow record batch builder for Vector events.
//!
//! This module provides functionality to convert a slice of Vector events into
//! Apache Arrow record batches using serde_arrow for automatic serialization.
//!
//! serde_arrow handles most type conversions automatically. The only manual
//! conversion needed is for timestamps, which must be converted to i64 values
//! in the schema's TimeUnit to avoid string parsing format mismatches.

use arrow::array::ArrayRef;
use arrow::compute::{CastOptions, cast_with_options};
use arrow::datatypes::{DataType, SchemaRef, TimeUnit};
use arrow::record_batch::RecordBatch;
use chrono::{DateTime, Utc};
use vector_core::event::{Event, LogEvent, Value};

use super::ArrowEncodingError;

/// Build an Arrow RecordBatch from a slice of events using the provided schema.
pub(crate) fn build_record_batch(
    schema: SchemaRef,
    events: &[Event],
) -> Result<RecordBatch, ArrowEncodingError> {
    let log_events: Vec<LogEvent> = events
        .iter()
        .filter_map(Event::maybe_as_log)
        .map(|log| convert_timestamps(log, &schema))
        .collect::<Result<Vec<_>, _>>()?;

    let batch = serde_arrow::to_record_batch(schema.fields(), &log_events)
        .map_err(|source| ArrowEncodingError::SerdeArrow { source })?;

    // Post-process: use Arrow's cast for any remaining type mismatches.
    // serde_arrow serializes Vector's Value types using fixed Arrow types (e.g., Int64
    // for all integers, Float64 for floats, LargeUtf8 for strings), but the target schema
    // may specify narrower types. Arrow's cast handles these conversions safely.
    let columns: Result<Vec<ArrayRef>, _> = batch
        .columns()
        .iter()
        .zip(schema.fields())
        .map(|(col, field)| {
            if col.data_type() == field.data_type() {
                Ok(col.clone())
            } else {
                cast_with_options(col, field.data_type(), &CastOptions::default())
                    .map_err(|source| ArrowEncodingError::RecordBatchCreation { source })
            }
        })
        .collect();

    RecordBatch::try_new(schema, columns?)
        .map_err(|source| ArrowEncodingError::RecordBatchCreation { source })
}

/// Convert Value::Timestamp to Value::Integer for timestamp columns.
///
/// This is necessary because serde_arrow's string parsing expects specific formats
/// based on the timezone setting, but Vector's timestamps always serialize as RFC 3339
/// with 'Z' suffix. Converting to i64 directly avoids this format mismatch.
fn convert_timestamps(
    event: &LogEvent,
    schema: &SchemaRef,
) -> Result<LogEvent, ArrowEncodingError> {
    let mut result = event.clone();

    for field in schema.fields() {
        if let DataType::Timestamp(unit, _) = field.data_type() {
            let field_name = field.name().as_str();

            if let Some(Value::Timestamp(ts)) = event.get(lookup::event_path!(field_name)) {
                let val = timestamp_to_unit(ts, unit).ok_or_else(|| {
                    ArrowEncodingError::TimestampOverflow {
                        field_name: field_name.to_string(),
                        timestamp: ts.to_rfc3339(),
                    }
                })?;
                result.insert(field_name, Value::Integer(val));
            }
        }
    }

    Ok(result)
}

/// Convert a DateTime<Utc> to i64 in the specified Arrow TimeUnit.
/// Returns None if the value would overflow (only possible for nanoseconds).
fn timestamp_to_unit(ts: &DateTime<Utc>, unit: &TimeUnit) -> Option<i64> {
    match unit {
        TimeUnit::Second => Some(ts.timestamp()),
        TimeUnit::Millisecond => Some(ts.timestamp_millis()),
        TimeUnit::Microsecond => Some(ts.timestamp_micros()),
        TimeUnit::Nanosecond => ts.timestamp_nanos_opt(),
    }
}
