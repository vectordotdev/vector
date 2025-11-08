//! Arrow IPC streaming format codec for batched event encoding
//!
//! Provides Apache Arrow IPC stream format encoding with static schema support.
//! This implements the streaming variant of the Arrow IPC protocol, which writes
//! a continuous stream of record batches without a file footer.

use arrow::{
    array::{
        ArrayRef, BinaryBuilder, BooleanBuilder, Decimal128Builder, Decimal256Builder,
        Float32Builder, Float64Builder, Int8Builder, Int16Builder, Int32Builder, Int64Builder,
        StringBuilder, TimestampMicrosecondBuilder, TimestampMillisecondBuilder,
        TimestampNanosecondBuilder, TimestampSecondBuilder, UInt8Builder, UInt16Builder,
        UInt32Builder, UInt64Builder,
    },
    datatypes::{DataType, Schema, TimeUnit, i256},
    ipc::writer::StreamWriter,
    record_batch::RecordBatch,
};
use bytes::{BufMut, Bytes, BytesMut};
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use snafu::Snafu;
use std::sync::Arc;
use tracing::debug;
use vector_config::configurable_component;

use vector_core::event::{Event, Value};

/// Configuration for Arrow IPC stream serialization
#[configurable_component]
#[derive(Clone, Default)]
pub struct ArrowStreamSerializerConfig {
    /// The Arrow schema to use for encoding
    #[serde(skip)]
    #[configurable(derived)]
    pub schema: Option<Arc<arrow::datatypes::Schema>>,
}

impl std::fmt::Debug for ArrowStreamSerializerConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ArrowStreamSerializerConfig")
            .field(
                "schema",
                &self
                    .schema
                    .as_ref()
                    .map(|s| format!("{} fields", s.fields().len())),
            )
            .finish()
    }
}

impl ArrowStreamSerializerConfig {
    /// Create a new ArrowStreamSerializerConfig with a schema
    pub fn new(schema: Arc<arrow::datatypes::Schema>) -> Self {
        Self {
            schema: Some(schema),
        }
    }

    /// The data type of events that are accepted by `ArrowStreamEncoder`.
    pub fn input_type(&self) -> vector_core::config::DataType {
        vector_core::config::DataType::Log
    }

    /// The schema required by the serializer.
    pub fn schema_requirement(&self) -> vector_core::schema::Requirement {
        vector_core::schema::Requirement::empty()
    }
}

/// Arrow IPC stream batch serializer that holds the schema
#[derive(Clone, Debug)]
pub struct ArrowStreamSerializer {
    schema: Arc<Schema>,
}

impl ArrowStreamSerializer {
    /// Create a new ArrowStreamSerializer with the given configuration
    pub fn new(config: ArrowStreamSerializerConfig) -> Result<Self, vector_common::Error> {
        let schema = config.schema.ok_or_else(|| {
            vector_common::Error::from(
                "Arrow serializer requires a schema. Pass a schema or fetch from provider before creating serializer."
            )
        })?;

        Ok(Self { schema })
    }
}

impl tokio_util::codec::Encoder<Vec<Event>> for ArrowStreamSerializer {
    type Error = vector_common::Error;

    fn encode(&mut self, events: Vec<Event>, buffer: &mut BytesMut) -> Result<(), Self::Error> {
        if events.is_empty() {
            return Err(vector_common::Error::from(
                "No events provided for encoding",
            ));
        }

        let bytes = encode_events_to_arrow_ipc_stream(&events, Some(Arc::clone(&self.schema)))
            .map_err(|e| vector_common::Error::from(e.to_string()))?;

        buffer.extend_from_slice(&bytes);
        Ok(())
    }
}

/// Errors that can occur during Arrow encoding
#[derive(Debug, Snafu)]
pub enum ArrowEncodingError {
    /// Failed to create Arrow record batch
    #[snafu(display("Failed to create Arrow record batch: {}", source))]
    RecordBatchCreation {
        /// The underlying Arrow error
        source: arrow::error::ArrowError,
    },

    /// Failed to write Arrow IPC data
    #[snafu(display("Failed to write Arrow IPC data: {}", source))]
    IpcWrite {
        /// The underlying Arrow error
        source: arrow::error::ArrowError,
    },

    /// No events provided for encoding
    #[snafu(display("No events provided for encoding"))]
    NoEvents,

    /// Schema must be provided before encoding
    #[snafu(display("Schema must be provided before encoding"))]
    NoSchemaProvided,

    /// Unsupported Arrow data type for field
    #[snafu(display(
        "Unsupported Arrow data type for field '{}': {:?}",
        field_name,
        data_type
    ))]
    UnsupportedType {
        /// The field name
        field_name: String,
        /// The unsupported data type
        data_type: DataType,
    },
}

/// Encodes a batch of events into Arrow IPC streaming format
pub fn encode_events_to_arrow_ipc_stream(
    events: &[Event],
    schema: Option<Arc<Schema>>,
) -> Result<Bytes, ArrowEncodingError> {
    if events.is_empty() {
        return Err(ArrowEncodingError::NoEvents);
    }

    let schema_ref = if let Some(provided_schema) = schema {
        provided_schema
    } else {
        return Err(ArrowEncodingError::NoSchemaProvided);
    };

    let record_batch = build_record_batch(Arc::<Schema>::clone(&schema_ref), events)?;

    debug!(
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
    debug!(
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
    let num_fields = schema.fields().len();
    let mut columns: Vec<ArrayRef> = Vec::with_capacity(num_fields);

    for field in schema.fields() {
        let field_name = field.name();
        let array: ArrayRef = match field.data_type() {
            DataType::Timestamp(time_unit, _) => {
                build_timestamp_array(events, field_name, *time_unit)?
            }
            DataType::Utf8 => build_string_array(events, field_name)?,
            DataType::Int8 => build_int8_array(events, field_name)?,
            DataType::Int16 => build_int16_array(events, field_name)?,
            DataType::Int32 => build_int32_array(events, field_name)?,
            DataType::Int64 => build_int64_array(events, field_name)?,
            DataType::UInt8 => build_uint8_array(events, field_name)?,
            DataType::UInt16 => build_uint16_array(events, field_name)?,
            DataType::UInt32 => build_uint32_array(events, field_name)?,
            DataType::UInt64 => build_uint64_array(events, field_name)?,
            DataType::Float32 => build_float32_array(events, field_name)?,
            DataType::Float64 => build_float64_array(events, field_name)?,
            DataType::Boolean => build_boolean_array(events, field_name)?,
            DataType::Binary => build_binary_array(events, field_name)?,
            DataType::Decimal128(precision, scale) => {
                build_decimal128_array(events, field_name, *precision, *scale)?
            }
            DataType::Decimal256(precision, scale) => {
                build_decimal256_array(events, field_name, *precision, *scale)?
            }
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

fn extract_timestamp(value: &Value) -> Option<DateTime<Utc>> {
    match value {
        Value::Timestamp(ts) => Some(*ts),
        Value::Bytes(bytes) => std::str::from_utf8(bytes)
            .ok()
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&Utc)),
        _ => None,
    }
}

fn build_timestamp_array(
    events: &[Event],
    field_name: &str,
    time_unit: TimeUnit,
) -> Result<ArrayRef, ArrowEncodingError> {
    macro_rules! build_array {
        ($builder:ty, $converter:expr) => {{
            let mut builder = <$builder>::new();
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

fn build_string_array(events: &[Event], field_name: &str) -> Result<ArrayRef, ArrowEncodingError> {
    let mut builder = StringBuilder::new();

    for event in events {
        if let Event::Log(log) = event {
            if let Some(value) = log.get(field_name) {
                match value {
                    Value::Bytes(bytes) => {
                        // Attempt direct UTF-8 conversion first, fallback to lossy
                        match std::str::from_utf8(bytes) {
                            Ok(s) => builder.append_value(s),
                            Err(_) => builder.append_value(&String::from_utf8_lossy(bytes)),
                        }
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

fn build_int8_array(events: &[Event], field_name: &str) -> Result<ArrayRef, ArrowEncodingError> {
    let mut builder = Int8Builder::new();

    for event in events {
        if let Event::Log(log) = event {
            match log.get(field_name) {
                Some(Value::Integer(i)) if *i >= i8::MIN as i64 && *i <= i8::MAX as i64 => {
                    builder.append_value(*i as i8)
                }
                _ => builder.append_null(),
            }
        }
    }

    Ok(Arc::new(builder.finish()))
}

fn build_int16_array(events: &[Event], field_name: &str) -> Result<ArrayRef, ArrowEncodingError> {
    let mut builder = Int16Builder::new();

    for event in events {
        if let Event::Log(log) = event {
            match log.get(field_name) {
                Some(Value::Integer(i)) if *i >= i16::MIN as i64 && *i <= i16::MAX as i64 => {
                    builder.append_value(*i as i16)
                }
                _ => builder.append_null(),
            }
        }
    }

    Ok(Arc::new(builder.finish()))
}

fn build_int32_array(events: &[Event], field_name: &str) -> Result<ArrayRef, ArrowEncodingError> {
    let mut builder = Int32Builder::new();

    for event in events {
        if let Event::Log(log) = event {
            match log.get(field_name) {
                Some(Value::Integer(i)) if *i >= i32::MIN as i64 && *i <= i32::MAX as i64 => {
                    builder.append_value(*i as i32)
                }
                _ => builder.append_null(),
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

fn build_uint8_array(events: &[Event], field_name: &str) -> Result<ArrayRef, ArrowEncodingError> {
    let mut builder = UInt8Builder::new();

    for event in events {
        if let Event::Log(log) = event {
            match log.get(field_name) {
                Some(Value::Integer(i)) if *i >= 0 && *i <= u8::MAX as i64 => {
                    builder.append_value(*i as u8)
                }
                _ => builder.append_null(),
            }
        }
    }

    Ok(Arc::new(builder.finish()))
}

fn build_uint16_array(events: &[Event], field_name: &str) -> Result<ArrayRef, ArrowEncodingError> {
    let mut builder = UInt16Builder::new();

    for event in events {
        if let Event::Log(log) = event {
            match log.get(field_name) {
                Some(Value::Integer(i)) if *i >= 0 && *i <= u16::MAX as i64 => {
                    builder.append_value(*i as u16)
                }
                _ => builder.append_null(),
            }
        }
    }

    Ok(Arc::new(builder.finish()))
}

fn build_uint32_array(events: &[Event], field_name: &str) -> Result<ArrayRef, ArrowEncodingError> {
    let mut builder = UInt32Builder::new();

    for event in events {
        if let Event::Log(log) = event {
            match log.get(field_name) {
                Some(Value::Integer(i)) if *i >= 0 && *i <= u32::MAX as i64 => {
                    builder.append_value(*i as u32)
                }
                _ => builder.append_null(),
            }
        }
    }

    Ok(Arc::new(builder.finish()))
}

fn build_uint64_array(events: &[Event], field_name: &str) -> Result<ArrayRef, ArrowEncodingError> {
    let mut builder = UInt64Builder::new();

    for event in events {
        if let Event::Log(log) = event {
            match log.get(field_name) {
                Some(Value::Integer(i)) if *i >= 0 => builder.append_value(*i as u64),
                _ => builder.append_null(),
            }
        }
    }

    Ok(Arc::new(builder.finish()))
}

fn build_float32_array(events: &[Event], field_name: &str) -> Result<ArrayRef, ArrowEncodingError> {
    let mut builder = Float32Builder::new();

    for event in events {
        if let Event::Log(log) = event {
            match log.get(field_name) {
                Some(Value::Float(f)) => builder.append_value(f.into_inner() as f32),
                Some(Value::Integer(i)) => builder.append_value(*i as f32),
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

fn build_decimal128_array(
    events: &[Event],
    field_name: &str,
    precision: u8,
    scale: i8,
) -> Result<ArrayRef, ArrowEncodingError> {
    let mut builder = Decimal128Builder::new()
        .with_precision_and_scale(precision, scale)
        .map_err(|_| ArrowEncodingError::UnsupportedType {
            field_name: field_name.to_string(),
            data_type: DataType::Decimal128(precision, scale),
        })?;

    let target_scale = scale.unsigned_abs() as u32;

    for event in events {
        if let Event::Log(log) = event {
            match log.get(field_name) {
                Some(Value::Float(f)) => {
                    if let Ok(mut decimal) = Decimal::try_from(f.into_inner()) {
                        decimal.rescale(target_scale);
                        let mantissa = decimal.mantissa();
                        builder.append_value(mantissa);
                    } else {
                        builder.append_null();
                    }
                }
                Some(Value::Integer(i)) => {
                    let mut decimal = Decimal::from(*i);
                    decimal.rescale(target_scale);
                    let mantissa = decimal.mantissa();
                    builder.append_value(mantissa);
                }
                _ => builder.append_null(),
            }
        }
    }

    Ok(Arc::new(builder.finish()))
}

fn build_decimal256_array(
    events: &[Event],
    field_name: &str,
    precision: u8,
    scale: i8,
) -> Result<ArrayRef, ArrowEncodingError> {
    let mut builder = Decimal256Builder::new()
        .with_precision_and_scale(precision, scale)
        .map_err(|_| ArrowEncodingError::UnsupportedType {
            field_name: field_name.to_string(),
            data_type: DataType::Decimal256(precision, scale),
        })?;

    let target_scale = scale.unsigned_abs() as u32;

    for event in events {
        if let Event::Log(log) = event {
            match log.get(field_name) {
                Some(Value::Float(f)) => {
                    if let Ok(mut decimal) = Decimal::try_from(f.into_inner()) {
                        decimal.rescale(target_scale);
                        let mantissa = decimal.mantissa();
                        // rust_decimal does not support i256 natively so we upcast here
                        builder.append_value(i256::from_i128(mantissa));
                    } else {
                        builder.append_null();
                    }
                }
                Some(Value::Integer(i)) => {
                    let mut decimal = Decimal::from(*i);
                    decimal.rescale(target_scale);
                    let mantissa = decimal.mantissa();
                    builder.append_value(i256::from_i128(mantissa));
                }
                _ => builder.append_null(),
            }
        }
    }

    Ok(Arc::new(builder.finish()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use arrow::{
        array::{
            Array, BinaryArray, BooleanArray, Float64Array, Int64Array, StringArray,
            TimestampMicrosecondArray, TimestampMillisecondArray, TimestampNanosecondArray,
            TimestampSecondArray,
        },
        datatypes::Field,
        ipc::reader::StreamReader,
    };
    use chrono::Utc;
    use std::io::Cursor;
    use vector_core::event::LogEvent;

    #[test]
    fn test_encode_all_types() {
        let mut log = LogEvent::default();
        log.insert("string_field", "test");
        log.insert("int8_field", 127);
        log.insert("int16_field", 32000);
        log.insert("int32_field", 1000000);
        log.insert("int64_field", 42);
        log.insert("float32_field", 3.14);
        log.insert("float64_field", 3.15);
        log.insert("bool_field", true);
        log.insert("bytes_field", bytes::Bytes::from("binary"));
        log.insert("timestamp_field", Utc::now());

        let events = vec![Event::Log(log)];

        let schema = Arc::new(Schema::new(vec![
            Field::new("string_field", DataType::Utf8, true),
            Field::new("int8_field", DataType::Int8, true),
            Field::new("int16_field", DataType::Int16, true),
            Field::new("int32_field", DataType::Int32, true),
            Field::new("int64_field", DataType::Int64, true),
            Field::new("float32_field", DataType::Float32, true),
            Field::new("float64_field", DataType::Float64, true),
            Field::new("bool_field", DataType::Boolean, true),
            Field::new("bytes_field", DataType::Binary, true),
            Field::new(
                "timestamp_field",
                DataType::Timestamp(TimeUnit::Millisecond, None),
                true,
            ),
        ]));

        let result = encode_events_to_arrow_ipc_stream(&events, Some(Arc::clone(&schema)));
        assert!(result.is_ok());

        let bytes = result.unwrap();
        let cursor = Cursor::new(bytes);
        let mut reader = StreamReader::try_new(cursor, None).unwrap();
        let batch = reader.next().unwrap().unwrap();

        assert_eq!(batch.num_rows(), 1);
        assert_eq!(batch.num_columns(), 10);

        // Verify string field
        assert_eq!(
            batch
                .column(0)
                .as_any()
                .downcast_ref::<StringArray>()
                .unwrap()
                .value(0),
            "test"
        );

        // Verify int8 field
        assert_eq!(
            batch
                .column(1)
                .as_any()
                .downcast_ref::<arrow::array::Int8Array>()
                .unwrap()
                .value(0),
            127
        );

        // Verify int16 field
        assert_eq!(
            batch
                .column(2)
                .as_any()
                .downcast_ref::<arrow::array::Int16Array>()
                .unwrap()
                .value(0),
            32000
        );

        // Verify int32 field
        assert_eq!(
            batch
                .column(3)
                .as_any()
                .downcast_ref::<arrow::array::Int32Array>()
                .unwrap()
                .value(0),
            1000000
        );

        // Verify int64 field
        assert_eq!(
            batch
                .column(4)
                .as_any()
                .downcast_ref::<Int64Array>()
                .unwrap()
                .value(0),
            42
        );

        // Verify float32 field
        assert!(
            (batch
                .column(5)
                .as_any()
                .downcast_ref::<arrow::array::Float32Array>()
                .unwrap()
                .value(0)
                - 3.14)
                .abs()
                < 0.001
        );

        // Verify float64 field
        assert!(
            (batch
                .column(6)
                .as_any()
                .downcast_ref::<Float64Array>()
                .unwrap()
                .value(0)
                - 3.15)
                .abs()
                < 0.001
        );

        // Verify boolean field
        assert!(
            batch
                .column(7)
                .as_any()
                .downcast_ref::<BooleanArray>()
                .unwrap()
                .value(0),
            "{}",
            true
        );

        // Verify binary field
        assert_eq!(
            batch
                .column(8)
                .as_any()
                .downcast_ref::<BinaryArray>()
                .unwrap()
                .value(0),
            b"binary"
        );

        // Verify timestamp field
        assert!(
            !batch
                .column(9)
                .as_any()
                .downcast_ref::<TimestampMillisecondArray>()
                .unwrap()
                .is_null(0)
        );
    }

    #[test]
    fn test_encode_null_values() {
        let mut log1 = LogEvent::default();
        log1.insert("field_a", 1);
        // field_b is missing

        let mut log2 = LogEvent::default();
        log2.insert("field_b", 2);
        // field_a is missing

        let events = vec![Event::Log(log1), Event::Log(log2)];

        let schema = Arc::new(Schema::new(vec![
            Field::new("field_a", DataType::Int64, true),
            Field::new("field_b", DataType::Int64, true),
        ]));

        let result = encode_events_to_arrow_ipc_stream(&events, Some(Arc::clone(&schema)));
        assert!(result.is_ok());

        let bytes = result.unwrap();
        let cursor = Cursor::new(bytes);
        let mut reader = StreamReader::try_new(cursor, None).unwrap();
        let batch = reader.next().unwrap().unwrap();

        assert_eq!(batch.num_rows(), 2);

        let field_a = batch
            .column(0)
            .as_any()
            .downcast_ref::<Int64Array>()
            .unwrap();
        assert_eq!(field_a.value(0), 1);
        assert!(field_a.is_null(1));

        let field_b = batch
            .column(1)
            .as_any()
            .downcast_ref::<Int64Array>()
            .unwrap();
        assert!(field_b.is_null(0));
        assert_eq!(field_b.value(1), 2);
    }

    #[test]
    fn test_encode_type_mismatches() {
        let mut log1 = LogEvent::default();
        log1.insert("field", 42); // Integer

        let mut log2 = LogEvent::default();
        log2.insert("field", 3.15); // Float - type mismatch!

        let events = vec![Event::Log(log1), Event::Log(log2)];

        // Schema expects Int64
        let schema = Arc::new(Schema::new(vec![Field::new(
            "field",
            DataType::Int64,
            true,
        )]));

        let result = encode_events_to_arrow_ipc_stream(&events, Some(Arc::clone(&schema)));
        assert!(result.is_ok());

        let bytes = result.unwrap();
        let cursor = Cursor::new(bytes);
        let mut reader = StreamReader::try_new(cursor, None).unwrap();
        let batch = reader.next().unwrap().unwrap();

        assert_eq!(batch.num_rows(), 2);

        let field_array = batch
            .column(0)
            .as_any()
            .downcast_ref::<Int64Array>()
            .unwrap();
        assert_eq!(field_array.value(0), 42);
        assert!(field_array.is_null(1)); // Type mismatch becomes null
    }

    #[test]
    fn test_encode_complex_json_values() {
        use serde_json::json;

        let mut log = LogEvent::default();
        log.insert(
            "object_field",
            json!({"key": "value", "nested": {"count": 42}}),
        );
        log.insert("array_field", json!([1, 2, 3]));

        let events = vec![Event::Log(log)];

        let schema = Arc::new(Schema::new(vec![
            Field::new("object_field", DataType::Utf8, true),
            Field::new("array_field", DataType::Utf8, true),
        ]));

        let result = encode_events_to_arrow_ipc_stream(&events, Some(Arc::clone(&schema)));
        assert!(result.is_ok());

        let bytes = result.unwrap();
        let cursor = Cursor::new(bytes);
        let mut reader = StreamReader::try_new(cursor, None).unwrap();
        let batch = reader.next().unwrap().unwrap();

        assert_eq!(batch.num_rows(), 1);

        let object_array = batch
            .column(0)
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap();
        let object_str = object_array.value(0);
        assert!(object_str.contains("key"));
        assert!(object_str.contains("value"));

        let array_array = batch
            .column(1)
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap();
        let array_str = array_array.value(0);
        assert_eq!(array_str, "[1,2,3]");
    }

    #[test]
    fn test_encode_unsupported_type() {
        let mut log = LogEvent::default();
        log.insert("field", "value");

        let events = vec![Event::Log(log)];

        // Use an unsupported type
        let schema = Arc::new(Schema::new(vec![Field::new(
            "field",
            DataType::Duration(TimeUnit::Millisecond),
            true,
        )]));

        let result = encode_events_to_arrow_ipc_stream(&events, Some(schema));
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            ArrowEncodingError::UnsupportedType { .. }
        ));
    }

    #[test]
    fn test_encode_without_schema_fails() {
        let mut log1 = LogEvent::default();
        log1.insert("message", "hello");

        let events = vec![Event::Log(log1)];

        let result = encode_events_to_arrow_ipc_stream(&events, None);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            ArrowEncodingError::NoSchemaProvided
        ));
    }

    #[test]
    fn test_encode_empty_events() {
        let events: Vec<Event> = vec![];
        let result = encode_events_to_arrow_ipc_stream(&events, None);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ArrowEncodingError::NoEvents));
    }

    #[test]
    fn test_encode_timestamp_precisions() {
        let now = Utc::now();
        let mut log = LogEvent::default();
        log.insert("ts_second", now);
        log.insert("ts_milli", now);
        log.insert("ts_micro", now);
        log.insert("ts_nano", now);

        let events = vec![Event::Log(log)];

        let schema = Arc::new(Schema::new(vec![
            Field::new(
                "ts_second",
                DataType::Timestamp(TimeUnit::Second, None),
                true,
            ),
            Field::new(
                "ts_milli",
                DataType::Timestamp(TimeUnit::Millisecond, None),
                true,
            ),
            Field::new(
                "ts_micro",
                DataType::Timestamp(TimeUnit::Microsecond, None),
                true,
            ),
            Field::new(
                "ts_nano",
                DataType::Timestamp(TimeUnit::Nanosecond, None),
                true,
            ),
        ]));

        let result = encode_events_to_arrow_ipc_stream(&events, Some(Arc::clone(&schema)));
        assert!(result.is_ok());

        let bytes = result.unwrap();
        let cursor = Cursor::new(bytes);
        let mut reader = StreamReader::try_new(cursor, None).unwrap();
        let batch = reader.next().unwrap().unwrap();

        assert_eq!(batch.num_rows(), 1);
        assert_eq!(batch.num_columns(), 4);

        let ts_second = batch
            .column(0)
            .as_any()
            .downcast_ref::<TimestampSecondArray>()
            .unwrap();
        assert!(!ts_second.is_null(0));
        assert_eq!(ts_second.value(0), now.timestamp());

        let ts_milli = batch
            .column(1)
            .as_any()
            .downcast_ref::<TimestampMillisecondArray>()
            .unwrap();
        assert!(!ts_milli.is_null(0));
        assert_eq!(ts_milli.value(0), now.timestamp_millis());

        let ts_micro = batch
            .column(2)
            .as_any()
            .downcast_ref::<TimestampMicrosecondArray>()
            .unwrap();
        assert!(!ts_micro.is_null(0));
        assert_eq!(ts_micro.value(0), now.timestamp_micros());

        let ts_nano = batch
            .column(3)
            .as_any()
            .downcast_ref::<TimestampNanosecondArray>()
            .unwrap();
        assert!(!ts_nano.is_null(0));
        assert_eq!(ts_nano.value(0), now.timestamp_nanos_opt().unwrap());
    }

    #[test]
    fn test_encode_mixed_timestamp_string_and_native() {
        // Test mixing string timestamps with native Timestamp values
        let mut log1 = LogEvent::default();
        log1.insert("ts", "2025-10-22T10:18:44.256Z"); // String

        let mut log2 = LogEvent::default();
        log2.insert("ts", Utc::now()); // Native Timestamp

        let mut log3 = LogEvent::default();
        log3.insert("ts", 1729594724256000000_i64); // Integer (nanoseconds)

        let events = vec![Event::Log(log1), Event::Log(log2), Event::Log(log3)];

        let schema = Arc::new(Schema::new(vec![Field::new(
            "ts",
            DataType::Timestamp(TimeUnit::Nanosecond, None),
            true,
        )]));

        let result = encode_events_to_arrow_ipc_stream(&events, Some(Arc::clone(&schema)));
        assert!(result.is_ok());

        let bytes = result.unwrap();
        let cursor = Cursor::new(bytes);
        let mut reader = StreamReader::try_new(cursor, None).unwrap();
        let batch = reader.next().unwrap().unwrap();

        assert_eq!(batch.num_rows(), 3);

        let ts_array = batch
            .column(0)
            .as_any()
            .downcast_ref::<TimestampNanosecondArray>()
            .unwrap();

        // All three should be non-null
        assert!(!ts_array.is_null(0));
        assert!(!ts_array.is_null(1));
        assert!(!ts_array.is_null(2));

        // First one should match the parsed string
        let expected = chrono::DateTime::parse_from_rfc3339("2025-10-22T10:18:44.256Z")
            .unwrap()
            .timestamp_nanos_opt()
            .unwrap();
        assert_eq!(ts_array.value(0), expected);

        // Third one should match the integer
        assert_eq!(ts_array.value(2), 1729594724256000000_i64);
    }

    #[test]
    fn test_encode_invalid_string_timestamp() {
        // Test that invalid timestamp strings become null
        let mut log1 = LogEvent::default();
        log1.insert("timestamp", "not-a-timestamp");

        let mut log2 = LogEvent::default();
        log2.insert("timestamp", "2025-10-22T10:18:44.256Z"); // Valid

        let mut log3 = LogEvent::default();
        log3.insert("timestamp", "2025-99-99T99:99:99Z"); // Invalid

        let events = vec![Event::Log(log1), Event::Log(log2), Event::Log(log3)];

        let schema = Arc::new(Schema::new(vec![Field::new(
            "timestamp",
            DataType::Timestamp(TimeUnit::Nanosecond, None),
            true,
        )]));

        let result = encode_events_to_arrow_ipc_stream(&events, Some(Arc::clone(&schema)));
        assert!(result.is_ok());

        let bytes = result.unwrap();
        let cursor = Cursor::new(bytes);
        let mut reader = StreamReader::try_new(cursor, None).unwrap();
        let batch = reader.next().unwrap().unwrap();

        assert_eq!(batch.num_rows(), 3);

        let ts_array = batch
            .column(0)
            .as_any()
            .downcast_ref::<TimestampNanosecondArray>()
            .unwrap();

        // Invalid timestamps should be null
        assert!(ts_array.is_null(0));
        assert!(!ts_array.is_null(1)); // Valid one
        assert!(ts_array.is_null(2));
    }

    #[test]
    fn test_encode_decimal128_from_integer() {
        use arrow::array::Decimal128Array;

        let mut log = LogEvent::default();
        // Store quantity as integer: 1000
        log.insert("quantity", 1000_i64);

        let events = vec![Event::Log(log)];

        // Decimal(10, 3) - will represent 1000 as 1000.000
        let schema = Arc::new(Schema::new(vec![Field::new(
            "quantity",
            DataType::Decimal128(10, 3),
            true,
        )]));

        let result = encode_events_to_arrow_ipc_stream(&events, Some(Arc::clone(&schema)));
        assert!(result.is_ok());

        let bytes = result.unwrap();
        let cursor = Cursor::new(bytes);
        let mut reader = StreamReader::try_new(cursor, None).unwrap();
        let batch = reader.next().unwrap().unwrap();

        assert_eq!(batch.num_rows(), 1);

        let decimal_array = batch
            .column(0)
            .as_any()
            .downcast_ref::<Decimal128Array>()
            .unwrap();

        assert!(!decimal_array.is_null(0));
        // 1000 with scale 3 = 1000 * 10^3 = 1000000
        assert_eq!(decimal_array.value(0), 1000000_i128);
    }

    #[test]
    fn test_encode_decimal256() {
        use arrow::array::Decimal256Array;

        let mut log = LogEvent::default();
        // Very large precision number
        log.insert("big_value", 123456789.123456_f64);

        let events = vec![Event::Log(log)];

        // Decimal256(50, 6) - high precision decimal
        let schema = Arc::new(Schema::new(vec![Field::new(
            "big_value",
            DataType::Decimal256(50, 6),
            true,
        )]));

        let result = encode_events_to_arrow_ipc_stream(&events, Some(Arc::clone(&schema)));
        assert!(result.is_ok());

        let bytes = result.unwrap();
        let cursor = Cursor::new(bytes);
        let mut reader = StreamReader::try_new(cursor, None).unwrap();
        let batch = reader.next().unwrap().unwrap();

        assert_eq!(batch.num_rows(), 1);

        let decimal_array = batch
            .column(0)
            .as_any()
            .downcast_ref::<Decimal256Array>()
            .unwrap();

        assert!(!decimal_array.is_null(0));
        // Value should be non-null and encoded
        let value = decimal_array.value(0);
        assert!(value.to_i128().is_some());
    }

    #[test]
    fn test_encode_decimal_null_values() {
        use arrow::array::Decimal128Array;

        let mut log1 = LogEvent::default();
        log1.insert("price", 99.99_f64);

        let log2 = LogEvent::default();
        // No price field - should be null

        let mut log3 = LogEvent::default();
        log3.insert("price", 50.00_f64);

        let events = vec![Event::Log(log1), Event::Log(log2), Event::Log(log3)];

        let schema = Arc::new(Schema::new(vec![Field::new(
            "price",
            DataType::Decimal128(10, 2),
            true,
        )]));

        let result = encode_events_to_arrow_ipc_stream(&events, Some(Arc::clone(&schema)));
        assert!(result.is_ok());

        let bytes = result.unwrap();
        let cursor = Cursor::new(bytes);
        let mut reader = StreamReader::try_new(cursor, None).unwrap();
        let batch = reader.next().unwrap().unwrap();

        assert_eq!(batch.num_rows(), 3);

        let decimal_array = batch
            .column(0)
            .as_any()
            .downcast_ref::<Decimal128Array>()
            .unwrap();

        // First row: 99.99
        assert!(!decimal_array.is_null(0));
        assert_eq!(decimal_array.value(0), 9999_i128);

        // Second row: null
        assert!(decimal_array.is_null(1));

        // Third row: 50.00
        assert!(!decimal_array.is_null(2));
        assert_eq!(decimal_array.value(2), 5000_i128);
    }

    #[test]
    fn test_encode_unsigned_integer_types() {
        use arrow::array::{UInt8Array, UInt16Array, UInt32Array, UInt64Array};

        let mut log = LogEvent::default();
        log.insert("uint8_field", 255_i64);
        log.insert("uint16_field", 65535_i64);
        log.insert("uint32_field", 4294967295_i64);
        log.insert("uint64_field", 9223372036854775807_i64);

        let events = vec![Event::Log(log)];

        let schema = Arc::new(Schema::new(vec![
            Field::new("uint8_field", DataType::UInt8, true),
            Field::new("uint16_field", DataType::UInt16, true),
            Field::new("uint32_field", DataType::UInt32, true),
            Field::new("uint64_field", DataType::UInt64, true),
        ]));

        let result = encode_events_to_arrow_ipc_stream(&events, Some(Arc::clone(&schema)));
        assert!(result.is_ok());

        let bytes = result.unwrap();
        let cursor = Cursor::new(bytes);
        let mut reader = StreamReader::try_new(cursor, None).unwrap();
        let batch = reader.next().unwrap().unwrap();

        assert_eq!(batch.num_rows(), 1);
        assert_eq!(batch.num_columns(), 4);

        // Verify uint8
        let uint8_array = batch
            .column(0)
            .as_any()
            .downcast_ref::<UInt8Array>()
            .unwrap();
        assert_eq!(uint8_array.value(0), 255_u8);

        // Verify uint16
        let uint16_array = batch
            .column(1)
            .as_any()
            .downcast_ref::<UInt16Array>()
            .unwrap();
        assert_eq!(uint16_array.value(0), 65535_u16);

        // Verify uint32
        let uint32_array = batch
            .column(2)
            .as_any()
            .downcast_ref::<UInt32Array>()
            .unwrap();
        assert_eq!(uint32_array.value(0), 4294967295_u32);

        // Verify uint64
        let uint64_array = batch
            .column(3)
            .as_any()
            .downcast_ref::<UInt64Array>()
            .unwrap();
        assert_eq!(uint64_array.value(0), 9223372036854775807_u64);
    }

    #[test]
    fn test_encode_unsigned_integers_with_null_and_overflow() {
        use arrow::array::{UInt8Array, UInt32Array};

        let mut log1 = LogEvent::default();
        log1.insert("uint8_field", 100_i64);
        log1.insert("uint32_field", 1000_i64);

        let mut log2 = LogEvent::default();
        log2.insert("uint8_field", 300_i64); // Overflow - should be null
        log2.insert("uint32_field", -1_i64); // Negative - should be null

        let log3 = LogEvent::default();
        // Missing fields - should be null

        let events = vec![Event::Log(log1), Event::Log(log2), Event::Log(log3)];

        let schema = Arc::new(Schema::new(vec![
            Field::new("uint8_field", DataType::UInt8, true),
            Field::new("uint32_field", DataType::UInt32, true),
        ]));

        let result = encode_events_to_arrow_ipc_stream(&events, Some(Arc::clone(&schema)));
        assert!(result.is_ok());

        let bytes = result.unwrap();
        let cursor = Cursor::new(bytes);
        let mut reader = StreamReader::try_new(cursor, None).unwrap();
        let batch = reader.next().unwrap().unwrap();

        assert_eq!(batch.num_rows(), 3);

        // Check uint8 column
        let uint8_array = batch
            .column(0)
            .as_any()
            .downcast_ref::<UInt8Array>()
            .unwrap();
        assert_eq!(uint8_array.value(0), 100_u8); // Valid
        assert!(uint8_array.is_null(1)); // Overflow
        assert!(uint8_array.is_null(2)); // Missing

        // Check uint32 column
        let uint32_array = batch
            .column(1)
            .as_any()
            .downcast_ref::<UInt32Array>()
            .unwrap();
        assert_eq!(uint32_array.value(0), 1000_u32); // Valid
        assert!(uint32_array.is_null(1)); // Negative
        assert!(uint32_array.is_null(2)); // Missing
    }
}
