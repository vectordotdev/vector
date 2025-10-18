//! Arrow encoding for Clickhouse sink events.

use std::sync::Arc;

use arrow::{
    array::{
        ArrayRef, BinaryBuilder, BooleanBuilder, Float64Builder, Int64Builder, StringBuilder,
        TimestampMillisecondBuilder,
    },
    datatypes::{DataType, Schema, TimeUnit},
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

    #[snafu(display(
        "Schema inference is not supported for ArrowStream format. Table schema must be known before insertion time."
    ))]
    NoSchemaProvided,

    #[snafu(display(
        "Unsupported Arrow data type for field '{}': {:?}",
        field_name,
        data_type
    ))]
    UnsupportedType {
        field_name: String,
        data_type: DataType,
    },
}

/// Encodes a batch of events into Arrow IPC format
pub fn encode_events_to_arrow_stream(
    events: &[Event],
    schema: Option<Arc<Schema>>,
) -> Result<Bytes, ArrowEncodingError> {
    if events.is_empty() {
        return Err(ArrowEncodingError::NoEvents);
    }

    // Use provided schema - schema inference is not supported
    let schema_ref = if let Some(provided_schema) = schema {
        tracing::debug!(
            "Using provided schema with {} fields",
            provided_schema.fields().len()
        );
        provided_schema
    } else {
        return Err(ArrowEncodingError::NoSchemaProvided);
    };

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
            other_type => {
                return Err(ArrowEncodingError::UnsupportedType {
                    field_name: field_name.to_string(),
                    data_type: other_type.clone(),
                });
            }
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
            match log.get(field_name) {
                Some(Value::Timestamp(ts)) => builder.append_value(ts.timestamp_millis()),
                _ => builder.append_null(),
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
                match value {
                    Value::Bytes(bytes) => {
                        builder.append_value(&String::from_utf8_lossy(bytes));
                    }
                    Value::Object(obj) => match serde_json::to_string(&obj) {
                        Ok(s) => builder.append_value(s),
                        Err(_) => builder.append_null(),
                    },
                    Value::Array(arr) => match serde_json::to_string(&arr) {
                        Ok(s) => builder.append_value(s),
                        Err(_) => builder.append_null(),
                    },
                    _ => {
                        builder.append_value(&value.to_string_lossy());
                    }
                }
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
            match log.get(field_name) {
                Some(Value::Integer(i)) => builder.append_value(*i),
                _ => builder.append_null(),
            }
        }
    }

    Ok(Arc::new(builder.finish()))
}

fn build_float64_array(events: &[Event], field_name: &str) -> Result<ArrayRef, ArrowEncodingError> {
    let mut builder = Float64Builder::new();

    for event in events {
        if let Event::Log(log) = event {
            match log.get(field_name) {
                Some(Value::Float(f)) => builder.append_value(f.into_inner()),
                Some(Value::Integer(i)) => builder.append_value(*i as f64),
                _ => builder.append_null(),
            }
        }
    }

    Ok(Arc::new(builder.finish()))
}

fn build_boolean_array(events: &[Event], field_name: &str) -> Result<ArrayRef, ArrowEncodingError> {
    let mut builder = BooleanBuilder::new();

    for event in events {
        if let Event::Log(log) = event {
            match log.get(field_name) {
                Some(Value::Boolean(b)) => builder.append_value(*b),
                _ => builder.append_null(),
            }
        }
    }

    Ok(Arc::new(builder.finish()))
}

fn build_binary_array(events: &[Event], field_name: &str) -> Result<ArrayRef, ArrowEncodingError> {
    let mut builder = BinaryBuilder::new();

    for event in events {
        if let Event::Log(log) = event {
            match log.get(field_name) {
                Some(Value::Bytes(bytes)) => builder.append_value(bytes),
                _ => builder.append_null(),
            }
        }
    }

    Ok(Arc::new(builder.finish()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::LogEvent;
    use arrow::datatypes::Field;

    #[test]
    fn test_encode_simple_events() {
        let mut log1 = LogEvent::default();
        log1.insert("message", "hello");
        log1.insert("count", 42);

        let mut log2 = LogEvent::default();
        log2.insert("message", "world");
        log2.insert("count", 100);

        let events = vec![Event::Log(log1), Event::Log(log2)];

        let schema = Arc::new(Schema::new(vec![
            Field::new("message", DataType::Utf8, true),
            Field::new("count", DataType::Int64, true),
        ]));

        let result = encode_events_to_arrow_stream(&events, Some(schema));
        assert!(result.is_ok());

        let bytes = result.unwrap();
        assert!(!bytes.is_empty());
    }

    #[test]
    fn test_encode_without_schema_fails() {
        let mut log1 = LogEvent::default();
        log1.insert("message", "hello");

        let events = vec![Event::Log(log1)];

        let result = encode_events_to_arrow_stream(&events, None);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            ArrowEncodingError::NoSchemaProvided
        ));
    }

    #[test]
    fn test_encode_empty_events() {
        let events: Vec<Event> = vec![];
        let result = encode_events_to_arrow_stream(&events, None);
        assert!(result.is_err());
    }
}
