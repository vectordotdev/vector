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
use async_trait::async_trait;
use bytes::{BufMut, Bytes, BytesMut};
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use snafu::Snafu;
use std::sync::Arc;
use vector_config::configurable_component;

use vector_core::event::{Event, Value};

/// Provides Arrow schema for encoding.
///
/// Sinks can implement this trait to provide custom schema fetching logic.
#[async_trait]
pub trait SchemaProvider: Send + Sync + std::fmt::Debug {
    /// Fetch the Arrow schema from the data store.
    ///
    /// This is called during sink configuration build phase to fetch
    /// the schema once at startup, rather than at runtime.
    async fn get_schema(&self) -> Result<Schema, ArrowEncodingError>;
}

/// Configuration for Arrow IPC stream serialization
#[configurable_component]
#[derive(Clone, Default)]
pub struct ArrowStreamSerializerConfig {
    /// The Arrow schema to use for encoding
    #[serde(skip)]
    #[configurable(derived)]
    pub schema: Option<arrow::datatypes::Schema>,

    /// Allow null values for non-nullable fields in the schema.
    ///
    /// When enabled, missing or incompatible values will be encoded as null even for fields
    /// marked as non-nullable in the Arrow schema. This is useful when working with downstream
    /// systems that can handle null values through defaults, computed columns, or other mechanisms.
    ///
    /// When disabled (default), missing values for non-nullable fields will cause encoding errors,
    /// ensuring all required data is present before sending to the sink.
    #[serde(default)]
    #[configurable(derived)]
    pub allow_nullable_fields: bool,
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
            .field("allow_nullable_fields", &self.allow_nullable_fields)
            .finish()
    }
}

impl ArrowStreamSerializerConfig {
    /// Create a new ArrowStreamSerializerConfig with a schema
    pub fn new(schema: arrow::datatypes::Schema) -> Self {
        Self {
            schema: Some(schema),
            allow_nullable_fields: false,
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
        let schema = config
            .schema
            .ok_or_else(|| vector_common::Error::from("Arrow serializer requires a schema."))?;

        // If allow_nullable_fields is enabled, transform the schema once here
        // instead of on every batch encoding
        let schema = if config.allow_nullable_fields {
            Schema::new_with_metadata(
                schema
                    .fields()
                    .iter()
                    .map(|f| Arc::new(make_field_nullable(f)))
                    .collect::<Vec<_>>(),
                schema.metadata().clone(),
            )
        } else {
            schema
        };

        Ok(Self {
            schema: Arc::new(schema),
        })
    }
}

impl tokio_util::codec::Encoder<Vec<Event>> for ArrowStreamSerializer {
    type Error = ArrowEncodingError;

    fn encode(&mut self, events: Vec<Event>, buffer: &mut BytesMut) -> Result<(), Self::Error> {
        if events.is_empty() {
            return Err(ArrowEncodingError::NoEvents);
        }

        let bytes = encode_events_to_arrow_ipc_stream(&events, Some(Arc::clone(&self.schema)))?;

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

    /// Failed to fetch schema from provider
    #[snafu(display("Failed to fetch schema from provider: {}", message))]
    SchemaFetchError {
        /// Error message from the provider
        message: String,
    },

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

    /// Null value encountered for non-nullable field
    #[snafu(display("Null value for non-nullable field '{}'", field_name))]
    NullConstraint {
        /// The field name
        field_name: String,
    },

    /// IO error during encoding
    #[snafu(display("IO error: {}", source))]
    Io {
        /// The underlying IO error
        source: std::io::Error,
    },
}

impl From<std::io::Error> for ArrowEncodingError {
    fn from(error: std::io::Error) -> Self {
        Self::Io { source: error }
    }
}

/// Encodes a batch of events into Arrow IPC streaming format
pub fn encode_events_to_arrow_ipc_stream(
    events: &[Event],
    schema: Option<Arc<Schema>>,
) -> Result<Bytes, ArrowEncodingError> {
    if events.is_empty() {
        return Err(ArrowEncodingError::NoEvents);
    }

    let schema_ref = schema.ok_or(ArrowEncodingError::NoSchemaProvided)?;

    let record_batch = build_record_batch(schema_ref, events)?;

    let ipc_err = |source| ArrowEncodingError::IpcWrite { source };

    let mut buffer = BytesMut::new().writer();
    let mut writer =
        StreamWriter::try_new(&mut buffer, record_batch.schema_ref()).map_err(ipc_err)?;
    writer.write(&record_batch).map_err(ipc_err)?;
    writer.finish().map_err(ipc_err)?;

    Ok(buffer.into_inner().freeze())
}

/// Recursively makes a Field and all its nested fields nullable
fn make_field_nullable(field: &arrow::datatypes::Field) -> arrow::datatypes::Field {
    let new_data_type = match field.data_type() {
        DataType::List(inner_field) => DataType::List(Arc::new(make_field_nullable(inner_field))),
        DataType::Struct(fields) => {
            DataType::Struct(fields.iter().map(|f| make_field_nullable(f)).collect())
        }
        DataType::Map(inner_field, sorted) => {
            DataType::Map(Arc::new(make_field_nullable(inner_field)), *sorted)
        }
        other => other.clone(),
    };

    field
        .clone()
        .with_data_type(new_data_type)
        .with_nullable(true)
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
        let nullable = field.is_nullable();
        let array: ArrayRef = match field.data_type() {
            DataType::Timestamp(time_unit, _) => {
                build_timestamp_array(events, field_name, *time_unit, nullable)?
            }
            DataType::Utf8 => build_string_array(events, field_name, nullable)?,
            DataType::Int8 => build_int8_array(events, field_name, nullable)?,
            DataType::Int16 => build_int16_array(events, field_name, nullable)?,
            DataType::Int32 => build_int32_array(events, field_name, nullable)?,
            DataType::Int64 => build_int64_array(events, field_name, nullable)?,
            DataType::UInt8 => build_uint8_array(events, field_name, nullable)?,
            DataType::UInt16 => build_uint16_array(events, field_name, nullable)?,
            DataType::UInt32 => build_uint32_array(events, field_name, nullable)?,
            DataType::UInt64 => build_uint64_array(events, field_name, nullable)?,
            DataType::Float32 => build_float32_array(events, field_name, nullable)?,
            DataType::Float64 => build_float64_array(events, field_name, nullable)?,
            DataType::Boolean => build_boolean_array(events, field_name, nullable)?,
            DataType::Binary => build_binary_array(events, field_name, nullable)?,
            DataType::Decimal128(precision, scale) => {
                build_decimal128_array(events, field_name, *precision, *scale, nullable)?
            }
            DataType::Decimal256(precision, scale) => {
                build_decimal256_array(events, field_name, *precision, *scale, nullable)?
            }
            other_type => {
                return Err(ArrowEncodingError::UnsupportedType {
                    field_name: field_name.into(),
                    data_type: other_type.clone(),
                });
            }
        };

        columns.push(array);
    }

    RecordBatch::try_new(schema, columns)
        .map_err(|source| ArrowEncodingError::RecordBatchCreation { source })
}

/// Macro to handle appending null or returning an error for non-nullable fields.
macro_rules! handle_null_constraints {
    ($builder:expr, $nullable:expr, $field_name:expr) => {{
        if !$nullable {
            return Err(ArrowEncodingError::NullConstraint {
                field_name: $field_name.into(),
            });
        }
        $builder.append_null();
    }};
}

/// Macro to generate a `build_*_array` function for primitive types.
macro_rules! define_build_primitive_array_fn {
    (
        $fn_name:ident, // The function name (e.g., build_int8_array)
        $builder_ty:ty, // The builder type (e.g., Int8Builder)
        // One or more match arms for valid Value types
        $( $value_pat:pat $(if $guard:expr)? => $append_expr:expr ),+
    ) => {
        fn $fn_name(
            events: &[Event],
            field_name: &str,
            nullable: bool,
        ) -> Result<ArrayRef, ArrowEncodingError> {
            let mut builder = <$builder_ty>::with_capacity(events.len());

            for event in events {
                if let Event::Log(log) = event {
                    match log.get(field_name) {
                        $(
                            $value_pat $(if $guard)? => builder.append_value($append_expr),
                        )+
                        // All other patterns are treated as null/invalid
                        _ => handle_null_constraints!(builder, nullable, field_name),
                    }
                }
            }
            Ok(Arc::new(builder.finish()))
        }
    };
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

fn build_string_array(
    events: &[Event],
    field_name: &str,
    nullable: bool,
) -> Result<ArrayRef, ArrowEncodingError> {
    let mut builder = StringBuilder::with_capacity(events.len(), 0);

    for event in events {
        if let Event::Log(log) = event {
            let mut appended = false;
            if let Some(value) = log.get(field_name) {
                match value {
                    Value::Bytes(bytes) => {
                        // Attempt direct UTF-8 conversion first, fallback to lossy
                        match std::str::from_utf8(bytes) {
                            Ok(s) => builder.append_value(s),
                            Err(_) => builder.append_value(&String::from_utf8_lossy(bytes)),
                        }
                        appended = true;
                    }
                    Value::Object(obj) => {
                        if let Ok(s) = serde_json::to_string(&obj) {
                            builder.append_value(s);
                            appended = true;
                        }
                    }
                    Value::Array(arr) => {
                        if let Ok(s) = serde_json::to_string(&arr) {
                            builder.append_value(s);
                            appended = true;
                        }
                    }
                    _ => {
                        builder.append_value(&value.to_string_lossy());
                        appended = true;
                    }
                }
            }

            if !appended {
                handle_null_constraints!(builder, nullable, field_name);
            }
        }
    }

    Ok(Arc::new(builder.finish()))
}

define_build_primitive_array_fn!(
    build_int8_array,
    Int8Builder,
    Some(Value::Integer(i)) if *i >= i8::MIN as i64 && *i <= i8::MAX as i64 => *i as i8
);

define_build_primitive_array_fn!(
    build_int16_array,
    Int16Builder,
    Some(Value::Integer(i)) if *i >= i16::MIN as i64 && *i <= i16::MAX as i64 => *i as i16
);

define_build_primitive_array_fn!(
    build_int32_array,
    Int32Builder,
    Some(Value::Integer(i)) if *i >= i32::MIN as i64 && *i <= i32::MAX as i64 => *i as i32
);

define_build_primitive_array_fn!(
    build_int64_array,
    Int64Builder,
    Some(Value::Integer(i)) => *i
);

define_build_primitive_array_fn!(
    build_uint8_array,
    UInt8Builder,
    Some(Value::Integer(i)) if *i >= 0 && *i <= u8::MAX as i64 => *i as u8
);

define_build_primitive_array_fn!(
    build_uint16_array,
    UInt16Builder,
    Some(Value::Integer(i)) if *i >= 0 && *i <= u16::MAX as i64 => *i as u16
);

define_build_primitive_array_fn!(
    build_uint32_array,
    UInt32Builder,
    Some(Value::Integer(i)) if *i >= 0 && *i <= u32::MAX as i64 => *i as u32
);

define_build_primitive_array_fn!(
    build_uint64_array,
    UInt64Builder,
    Some(Value::Integer(i)) if *i >= 0 => *i as u64
);

define_build_primitive_array_fn!(
    build_float32_array,
    Float32Builder,
    Some(Value::Float(f)) => f.into_inner() as f32,
    Some(Value::Integer(i)) => *i as f32
);

define_build_primitive_array_fn!(
    build_float64_array,
    Float64Builder,
    Some(Value::Float(f)) => f.into_inner(),
    Some(Value::Integer(i)) => *i as f64
);

define_build_primitive_array_fn!(
    build_boolean_array,
    BooleanBuilder,
    Some(Value::Boolean(b)) => *b
);

fn build_binary_array(
    events: &[Event],
    field_name: &str,
    nullable: bool,
) -> Result<ArrayRef, ArrowEncodingError> {
    let mut builder = BinaryBuilder::with_capacity(events.len(), 0);

    for event in events {
        if let Event::Log(log) = event {
            match log.get(field_name) {
                Some(Value::Bytes(bytes)) => builder.append_value(bytes),
                _ => handle_null_constraints!(builder, nullable, field_name),
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
    nullable: bool,
) -> Result<ArrayRef, ArrowEncodingError> {
    let mut builder = Decimal128Builder::with_capacity(events.len())
        .with_precision_and_scale(precision, scale)
        .map_err(|_| ArrowEncodingError::UnsupportedType {
            field_name: field_name.into(),
            data_type: DataType::Decimal128(precision, scale),
        })?;

    let target_scale = scale.unsigned_abs() as u32;

    for event in events {
        if let Event::Log(log) = event {
            let mut appended = false;
            match log.get(field_name) {
                Some(Value::Float(f)) => {
                    if let Ok(mut decimal) = Decimal::try_from(f.into_inner()) {
                        decimal.rescale(target_scale);
                        let mantissa = decimal.mantissa();
                        builder.append_value(mantissa);
                        appended = true;
                    }
                }
                Some(Value::Integer(i)) => {
                    let mut decimal = Decimal::from(*i);
                    decimal.rescale(target_scale);
                    let mantissa = decimal.mantissa();
                    builder.append_value(mantissa);
                    appended = true;
                }
                _ => {}
            }

            if !appended {
                handle_null_constraints!(builder, nullable, field_name);
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
    nullable: bool,
) -> Result<ArrayRef, ArrowEncodingError> {
    let mut builder = Decimal256Builder::with_capacity(events.len())
        .with_precision_and_scale(precision, scale)
        .map_err(|_| ArrowEncodingError::UnsupportedType {
            field_name: field_name.into(),
            data_type: DataType::Decimal256(precision, scale),
        })?;

    let target_scale = scale.unsigned_abs() as u32;

    for event in events {
        if let Event::Log(log) = event {
            let mut appended = false;
            match log.get(field_name) {
                Some(Value::Float(f)) => {
                    if let Ok(mut decimal) = Decimal::try_from(f.into_inner()) {
                        decimal.rescale(target_scale);
                        let mantissa = decimal.mantissa();
                        // rust_decimal does not support i256 natively so we upcast here
                        builder.append_value(i256::from_i128(mantissa));
                        appended = true;
                    }
                }
                Some(Value::Integer(i)) => {
                    let mut decimal = Decimal::from(*i);
                    decimal.rescale(target_scale);
                    let mantissa = decimal.mantissa();
                    builder.append_value(i256::from_i128(mantissa));
                    appended = true;
                }
                _ => {}
            }

            if !appended {
                handle_null_constraints!(builder, nullable, field_name);
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
        log.insert("float32_field", 3.15);
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
                - 3.15)
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

    #[test]
    fn test_encode_non_nullable_field_with_null_value() {
        // Test that encoding fails when a non-nullable field encounters a null value
        let mut log1 = LogEvent::default();
        log1.insert("required_field", 42);

        let log2 = LogEvent::default();
        // log2 is missing required_field - should cause an error

        let events = vec![Event::Log(log1), Event::Log(log2)];

        // Create schema with non-nullable field
        let schema = Arc::new(Schema::new(vec![Field::new(
            "required_field",
            DataType::Int64,
            false, // Not nullable
        )]));

        let result = encode_events_to_arrow_ipc_stream(&events, Some(schema));
        assert!(result.is_err());

        match result.unwrap_err() {
            ArrowEncodingError::NullConstraint { field_name } => {
                assert_eq!(field_name, "required_field");
            }
            other => panic!("Expected NullConstraint error, got: {:?}", other),
        }
    }

    #[test]
    fn test_encode_non_nullable_string_field_with_missing_value() {
        // Test that encoding fails for non-nullable string field
        let mut log1 = LogEvent::default();
        log1.insert("name", "Alice");

        let mut log2 = LogEvent::default();
        log2.insert("name", "Bob");

        let log3 = LogEvent::default();
        // log3 is missing name field

        let events = vec![Event::Log(log1), Event::Log(log2), Event::Log(log3)];

        let schema = Arc::new(Schema::new(vec![Field::new(
            "name",
            DataType::Utf8,
            false, // Not nullable
        )]));

        let result = encode_events_to_arrow_ipc_stream(&events, Some(schema));
        assert!(result.is_err());

        match result.unwrap_err() {
            ArrowEncodingError::NullConstraint { field_name } => {
                assert_eq!(field_name, "name");
            }
            other => panic!("Expected NullConstraint error, got: {:?}", other),
        }
    }

    #[test]
    fn test_encode_non_nullable_field_all_values_present() {
        // Test that encoding succeeds when all values are present for non-nullable field
        let mut log1 = LogEvent::default();
        log1.insert("id", 1);

        let mut log2 = LogEvent::default();
        log2.insert("id", 2);

        let mut log3 = LogEvent::default();
        log3.insert("id", 3);

        let events = vec![Event::Log(log1), Event::Log(log2), Event::Log(log3)];

        let schema = Arc::new(Schema::new(vec![Field::new(
            "id",
            DataType::Int64,
            false, // Not nullable
        )]));

        let result = encode_events_to_arrow_ipc_stream(&events, Some(Arc::clone(&schema)));
        assert!(result.is_ok());

        let bytes = result.unwrap();
        let cursor = Cursor::new(bytes);
        let mut reader = StreamReader::try_new(cursor, None).unwrap();
        let batch = reader.next().unwrap().unwrap();

        assert_eq!(batch.num_rows(), 3);

        let id_array = batch
            .column(0)
            .as_any()
            .downcast_ref::<Int64Array>()
            .unwrap();

        assert_eq!(id_array.value(0), 1);
        assert_eq!(id_array.value(1), 2);
        assert_eq!(id_array.value(2), 3);
        assert!(!id_array.is_null(0));
        assert!(!id_array.is_null(1));
        assert!(!id_array.is_null(2));
    }

    #[test]
    fn test_config_allow_nullable_fields_overrides_schema() {
        use tokio_util::codec::Encoder;

        // Create events: One valid, one missing the "required" field
        let mut log1 = LogEvent::default();
        log1.insert("strict_field", 42);
        let log2 = LogEvent::default();
        let events = vec![Event::Log(log1), Event::Log(log2)];

        let schema = Schema::new(vec![Field::new("strict_field", DataType::Int64, false)]);

        let mut config = ArrowStreamSerializerConfig::new(schema);
        config.allow_nullable_fields = true;

        let mut serializer =
            ArrowStreamSerializer::new(config).expect("Failed to create serializer");

        let mut buffer = BytesMut::new();
        serializer
            .encode(events, &mut buffer)
            .expect("Encoding should succeed when allow_nullable_fields is true");

        let cursor = Cursor::new(buffer);
        let mut reader = StreamReader::try_new(cursor, None).expect("Failed to create reader");
        let batch = reader.next().unwrap().expect("Failed to read batch");

        assert_eq!(batch.num_rows(), 2);

        let binding = batch.schema();
        let output_field = binding.field(0);
        assert!(
            output_field.is_nullable(),
            "The output schema field should have been transformed to nullable=true"
        );

        let array = batch
            .column(0)
            .as_any()
            .downcast_ref::<Int64Array>()
            .unwrap();

        assert_eq!(array.value(0), 42);
        assert!(!array.is_null(0));
        assert!(
            array.is_null(1),
            "The missing value should be encoded as null"
        );
    }

    #[test]
    fn test_make_field_nullable_with_nested_types() {
        // Test that make_field_nullable recursively handles List and Struct types

        // Create a nested structure: Struct containing a List of Structs
        // struct { inner_list: [{ nested_field: Int64 }] }
        let inner_struct_field = Field::new("nested_field", DataType::Int64, false);
        let inner_struct =
            DataType::Struct(arrow::datatypes::Fields::from(vec![inner_struct_field]));
        let list_field = Field::new("item", inner_struct, false);
        let list_type = DataType::List(Arc::new(list_field));
        let outer_field = Field::new("inner_list", list_type, false);
        let outer_struct = DataType::Struct(arrow::datatypes::Fields::from(vec![outer_field]));

        let original_field = Field::new("root", outer_struct, false);

        // Apply make_field_nullable
        let nullable_field = make_field_nullable(&original_field);

        // Verify root field is nullable
        assert!(
            nullable_field.is_nullable(),
            "Root field should be nullable"
        );

        // Verify nested struct is nullable
        if let DataType::Struct(root_fields) = nullable_field.data_type() {
            let inner_list_field = &root_fields[0];
            assert!(
                inner_list_field.is_nullable(),
                "inner_list field should be nullable"
            );

            // Verify list element is nullable
            if let DataType::List(list_item_field) = inner_list_field.data_type() {
                assert!(
                    list_item_field.is_nullable(),
                    "List item field should be nullable"
                );

                // Verify inner struct fields are nullable
                if let DataType::Struct(inner_struct_fields) = list_item_field.data_type() {
                    let nested_field = &inner_struct_fields[0];
                    assert!(
                        nested_field.is_nullable(),
                        "nested_field should be nullable"
                    );
                } else {
                    panic!("Expected Struct type for list items");
                }
            } else {
                panic!("Expected List type for inner_list");
            }
        } else {
            panic!("Expected Struct type for root field");
        }
    }

    #[test]
    fn test_make_field_nullable_with_map_type() {
        // Test that make_field_nullable handles Map types
        // Map is internally represented as List<Struct<key, value>>

        // Create a map: Map<Utf8, Int64>
        // Internally: List<Struct<entries: {key: Utf8, value: Int64}>>
        let key_field = Field::new("key", DataType::Utf8, false);
        let value_field = Field::new("value", DataType::Int64, false);
        let entries_struct =
            DataType::Struct(arrow::datatypes::Fields::from(vec![key_field, value_field]));
        let entries_field = Field::new("entries", entries_struct, false);
        let map_type = DataType::Map(Arc::new(entries_field), false);

        let original_field = Field::new("my_map", map_type, false);

        // Apply make_field_nullable
        let nullable_field = make_field_nullable(&original_field);

        // Verify root field is nullable
        assert!(
            nullable_field.is_nullable(),
            "Root map field should be nullable"
        );

        // Verify map entries are nullable
        if let DataType::Map(entries_field, _sorted) = nullable_field.data_type() {
            assert!(
                entries_field.is_nullable(),
                "Map entries field should be nullable"
            );

            // Verify the struct inside the map is nullable
            if let DataType::Struct(struct_fields) = entries_field.data_type() {
                let key_field = &struct_fields[0];
                let value_field = &struct_fields[1];
                assert!(key_field.is_nullable(), "Map key field should be nullable");
                assert!(
                    value_field.is_nullable(),
                    "Map value field should be nullable"
                );
            } else {
                panic!("Expected Struct type for map entries");
            }
        } else {
            panic!("Expected Map type for my_map field");
        }
    }
}
