//! Arrow encoding for Clickhouse sink events.

use std::sync::Arc;

use arrow::{
    array::{
        ArrayRef, BinaryBuilder, BooleanBuilder, Float64Builder, Int64Builder, StringBuilder,
        TimestampMillisecondBuilder,
    },
    datatypes::{DataType, Field, Schema, TimeUnit},
    ipc::writer::StreamWriter,
    record_batch::RecordBatch,
};
use bytes::{BufMut, Bytes, BytesMut};
use snafu::Snafu;

use crate::event::{Event, Value};

#[derive(Debug, Snafu)]
pub enum ArrowEncodingError {
    #[snafu(display("Failed to create Arrow record batch: {}", source))]
    RecordBatchCreation { source: arrow::error::ArrowError },

    #[snafu(display("Failed to write Arrow IPC data: {}", source))]
    IpcWrite { source: arrow::error::ArrowError },

    #[snafu(display("No events provided for encoding"))]
    NoEvents,
}

/// Encodes a batch of events into Arrow IPC format
pub fn encode_events_to_arrow_stream(events: &[Event]) -> Result<Bytes, ArrowEncodingError> {
    if events.is_empty() {
        return Err(ArrowEncodingError::NoEvents);
    }

    tracing::debug!(
        "Encoding {} events to Arrow IPC stream format",
        events.len()
    );

    // Build schema from first event's structure
    let schema = build_schema_from_events(events)?;
    let schema_ref = Arc::new(schema);

    tracing::debug!(
        "Built Arrow schema with {} fields: {:?}",
        schema_ref.fields().len(),
        schema_ref
            .fields()
            .iter()
            .map(|f| format!("{}:{:?}", f.name(), f.data_type()))
            .collect::<Vec<_>>()
    );

    // Build record batch from events
    let record_batch = build_record_batch(Arc::<Schema>::clone(&schema_ref), events)?;

    tracing::debug!(
        "Built RecordBatch with {} rows and {} columns",
        record_batch.num_rows(),
        record_batch.num_columns()
    );

    // Encode to Arrow IPC format
    let mut buffer = BytesMut::new().writer();
    {
        let mut writer = StreamWriter::try_new(&mut buffer, &schema_ref)
            .map_err(|source| ArrowEncodingError::IpcWrite { source })?;

        writer
            .write(&record_batch)
            .map_err(|source| ArrowEncodingError::IpcWrite { source })?;

        writer
            .finish()
            .map_err(|source| ArrowEncodingError::IpcWrite { source })?;
    }

    let encoded_bytes = buffer.into_inner().freeze();
    tracing::debug!(
        "Encoded to {} bytes of Arrow IPC stream data",
        encoded_bytes.len()
    );

    Ok(encoded_bytes)
}

/// Builds an Arrow schema from the structure of events
fn build_schema_from_events(events: &[Event]) -> Result<Schema, ArrowEncodingError> {
    let mut fields = Vec::new();

    // Collect field names from all events
    let mut field_names = std::collections::BTreeSet::new();
    for event in events {
        if let Event::Log(log) = event {
            for (key, _) in log.all_event_fields().expect("Failed to get event fields") {
                field_names.insert(key.to_string());
            }
        }
    }

    // Create fields for each unique field name
    for field_name in field_names {
        // Infer type from first occurrence
        let field_type = infer_field_type(events, &field_name);
        fields.push(Field::new(&field_name, field_type, true));
    }

    Ok(Schema::new(fields))
}

/// Infers the Arrow data type for a field based on the first non-null value found
fn infer_field_type(events: &[Event], field_name: &str) -> DataType {
    for event in events {
        if let Event::Log(log) = event
            && let Some(value) = log.get(field_name)
        {
            return match value {
                Value::Bytes(_) => DataType::Utf8,
                Value::Integer(_) => DataType::Int64,
                Value::Float(_) => DataType::Float64,
                Value::Boolean(_) => DataType::Boolean,
                Value::Timestamp(_) => DataType::Timestamp(TimeUnit::Millisecond, None),
                Value::Object(_) => DataType::Utf8, // Serialize as JSON string
                Value::Array(_) => DataType::Utf8,  // Serialize as JSON string
                _ => DataType::Utf8,                // Default to string
            };
        }
    }
    DataType::Utf8 // Default to string if no value found
}

/// Builds an Arrow RecordBatch from events
fn build_record_batch(
    schema: Arc<Schema>,
    events: &[Event],
) -> Result<RecordBatch, ArrowEncodingError> {
    let mut columns: Vec<ArrayRef> = Vec::new();

    for field in schema.fields() {
        let field_name = field.name();
        let array: ArrayRef = match field.data_type() {
            DataType::Timestamp(TimeUnit::Millisecond, _) => {
                build_timestamp_array(events, field_name)?
            }
            DataType::Utf8 => build_string_array(events, field_name)?,
            DataType::Int64 => build_int64_array(events, field_name)?,
            DataType::Float64 => build_float64_array(events, field_name)?,
            DataType::Boolean => build_boolean_array(events, field_name)?,
            DataType::Binary => build_binary_array(events, field_name)?,
            _ => build_string_array(events, field_name)?, // Fallback to string
        };

        columns.push(array);
    }

    RecordBatch::try_new(schema, columns)
        .map_err(|source| ArrowEncodingError::RecordBatchCreation { source })
}

fn build_timestamp_array(
    events: &[Event],
    field_name: &str,
) -> Result<ArrayRef, ArrowEncodingError> {
    let mut builder = TimestampMillisecondBuilder::new();

    for event in events {
        if let Event::Log(log) = event {
            if let Some(value) = log.get(field_name) {
                match value {
                    Value::Timestamp(ts) => builder.append_value(ts.timestamp_millis()),
                    _ => builder.append_null(),
                }
            } else {
                builder.append_null();
            }
        }
    }

    Ok(Arc::new(builder.finish()))
}

fn build_string_array(events: &[Event], field_name: &str) -> Result<ArrayRef, ArrowEncodingError> {
    let mut builder = StringBuilder::new();

    for event in events {
        if let Event::Log(log) = event {
            if let Some(value) = log.get(field_name) {
                let string_value = match value {
                    Value::Bytes(bytes) => String::from_utf8_lossy(bytes).to_string(),
                    Value::Object(obj) => serde_json::to_string(&obj).unwrap_or_default(),
                    Value::Array(arr) => serde_json::to_string(&arr).unwrap_or_default(),
                    _ => value.to_string_lossy().to_string(),
                };
                builder.append_value(string_value);
            } else {
                builder.append_null();
            }
        }
    }

    Ok(Arc::new(builder.finish()))
}

fn build_int64_array(events: &[Event], field_name: &str) -> Result<ArrayRef, ArrowEncodingError> {
    let mut builder = Int64Builder::new();

    for event in events {
        if let Event::Log(log) = event {
            if let Some(value) = log.get(field_name) {
                match value {
                    Value::Integer(i) => builder.append_value(*i),
                    Value::Float(f) => builder.append_value(f.into_inner() as i64),
                    _ => builder.append_null(),
                }
            } else {
                builder.append_null();
            }
        }
    }

    Ok(Arc::new(builder.finish()))
}

fn build_float64_array(events: &[Event], field_name: &str) -> Result<ArrayRef, ArrowEncodingError> {
    let mut builder = Float64Builder::new();

    for event in events {
        if let Event::Log(log) = event {
            if let Some(value) = log.get(field_name) {
                match value {
                    Value::Float(f) => builder.append_value(f.into_inner()),
                    Value::Integer(i) => builder.append_value(*i as f64),
                    _ => builder.append_null(),
                }
            } else {
                builder.append_null();
            }
        }
    }

    Ok(Arc::new(builder.finish()))
}

fn build_boolean_array(events: &[Event], field_name: &str) -> Result<ArrayRef, ArrowEncodingError> {
    let mut builder = BooleanBuilder::new();

    for event in events {
        if let Event::Log(log) = event {
            if let Some(value) = log.get(field_name) {
                match value {
                    Value::Boolean(b) => builder.append_value(*b),
                    _ => builder.append_null(),
                }
            } else {
                builder.append_null();
            }
        }
    }

    Ok(Arc::new(builder.finish()))
}

fn build_binary_array(events: &[Event], field_name: &str) -> Result<ArrayRef, ArrowEncodingError> {
    let mut builder = BinaryBuilder::new();

    for event in events {
        if let Event::Log(log) = event {
            if let Some(value) = log.get(field_name) {
                match value {
                    Value::Bytes(bytes) => builder.append_value(bytes),
                    _ => builder.append_null(),
                }
            } else {
                builder.append_null();
            }
        }
    }

    Ok(Arc::new(builder.finish()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::LogEvent;

    #[test]
    fn test_encode_simple_events() {
        let mut log1 = LogEvent::default();
        log1.insert("message", "hello");
        log1.insert("count", 42);

        let mut log2 = LogEvent::default();
        log2.insert("message", "world");
        log2.insert("count", 100);

        let events = vec![Event::Log(log1), Event::Log(log2)];

        let result = encode_events_to_arrow_stream(&events);
        assert!(result.is_ok());

        let bytes = result.unwrap();
        assert!(!bytes.is_empty());
    }

    #[test]
    fn test_encode_empty_events() {
        let events: Vec<Event> = vec![];
        let result = encode_events_to_arrow_stream(&events);
        assert!(result.is_err());
    }
}
