//! Arrow IPC streaming format codec for batched event encoding
//!
//! Provides Apache Arrow IPC stream format encoding with static schema support.
//! This implements the streaming variant of the Arrow IPC protocol, which writes
//! a continuous stream of record batches without a file footer.

use arrow::{
    array::ArrayRef,
    compute::{CastOptions, cast_with_options},
    datatypes::{DataType, Field, Fields, Schema, SchemaRef},
    ipc::writer::StreamWriter,
    json::reader::{Decoder, ReaderBuilder},
    record_batch::RecordBatch,
};
use async_trait::async_trait;
use bytes::{BufMut, Bytes, BytesMut};
use snafu::Snafu;
use std::sync::Arc;
use vector_config::configurable_component;
use vector_core::event::Event;

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
    schema: SchemaRef,
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
            let nullable_fields: Fields = schema
                .fields()
                .iter()
                .map(|f| make_field_nullable(f))
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| vector_common::Error::from(e.to_string()))?
                .into();
            Schema::new_with_metadata(nullable_fields, schema.metadata().clone())
        } else {
            schema
        };

        Ok(Self {
            schema: SchemaRef::new(schema),
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

    /// Arrow JSON decoding error
    #[snafu(display("Arrow JSON decoding error: {}", source))]
    ArrowJsonDecode {
        /// The underlying Arrow error
        source: arrow::error::ArrowError,
    },

    /// Invalid Map schema structure
    #[snafu(display("Invalid Map schema for field '{}': {}", field_name, reason))]
    InvalidMapSchema {
        /// The field name
        field_name: String,
        /// Description of the schema violation
        reason: String,
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
    schema: Option<SchemaRef>,
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
fn make_field_nullable(field: &Field) -> Result<Field, ArrowEncodingError> {
    let new_data_type = match field.data_type() {
        DataType::List(inner_field) => DataType::List(make_field_nullable(inner_field)?.into()),
        DataType::Struct(fields) => DataType::Struct(
            fields
                .iter()
                .map(|f| make_field_nullable(f))
                .collect::<Result<Vec<_>, _>>()?
                .into(),
        ),
        DataType::Map(inner, sorted) => {
            // A Map's inner field is a "entries" Struct<Key, Value>
            let DataType::Struct(fields) = inner.data_type() else {
                return Err(ArrowEncodingError::InvalidMapSchema {
                    field_name: field.name().to_string(),
                    reason: format!("inner type must be Struct, found {:?}", inner.data_type()),
                });
            };

            if fields.len() != 2 {
                return Err(ArrowEncodingError::InvalidMapSchema {
                    field_name: field.name().to_string(),
                    reason: format!("expected 2 fields (key, value), found {}", fields.len()),
                });
            }
            let key_field = &fields[0];
            let value_field = &fields[1];

            let new_struct_fields: Fields =
                [key_field.clone(), make_field_nullable(value_field)?.into()].into();

            // Reconstruct the inner "entries" field
            // The inner field itself must be non-nullable (only the Map wrapper is nullable)
            let new_inner_field = inner
                .as_ref()
                .clone()
                .with_data_type(DataType::Struct(new_struct_fields))
                .with_nullable(false);

            DataType::Map(new_inner_field.into(), *sorted)
        }
        other => other.clone(),
    };

    Ok(field
        .clone()
        .with_data_type(new_data_type)
        .with_nullable(true))
}

/// Build a decoder schema: swap Binary fields to Utf8 so arrow-json can decode
/// Vector's UTF-8 serialized bytes (arrow-json expects hex-encoded strings for Binary).
fn build_decoder_schema(schema: &Schema) -> Schema {
    let fields: Vec<Field> = schema
        .fields()
        .iter()
        .map(|f| match f.data_type() {
            DataType::Binary => f.as_ref().clone().with_data_type(DataType::Utf8),
            _ => f.as_ref().clone(),
        })
        .collect();
    Schema::new_with_metadata(Fields::from(fields), schema.metadata().clone())
}

/// Build an Arrow RecordBatch from a slice of events using the provided schema.
fn build_record_batch(
    schema: SchemaRef,
    events: &[Event],
) -> Result<RecordBatch, ArrowEncodingError> {
    // Pre-validate non-nullable fields (arrow-json silently writes defaults for missing fields)
    validate_non_nullable_fields(events, &schema)?;

    let decoder_schema = build_decoder_schema(&schema);
    let decoder = ReaderBuilder::new(Arc::new(decoder_schema))
        .build_decoder()
        .map_err(|source| ArrowEncodingError::RecordBatchCreation { source })?;

    let batch = decode_events(decoder, events)?;

    // Post-process: cast columns that were swapped for decoder compatibility
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

/// Validate that non-nullable fields are present in all events.
fn validate_non_nullable_fields(
    events: &[Event],
    schema: &SchemaRef,
) -> Result<(), ArrowEncodingError> {
    let required_fields: Vec<&str> = schema
        .fields()
        .iter()
        .filter(|f| !f.is_nullable())
        .map(|f| f.name().as_str())
        .collect();

    for name in required_fields {
        if events
            .iter()
            .filter_map(Event::maybe_as_log)
            .any(|log| log.get(lookup::event_path!(name)).is_none())
        {
            return Err(ArrowEncodingError::NullConstraint {
                field_name: name.to_string(),
            });
        }
    }
    Ok(())
}

/// Serialize events as JSON and decode them into a RecordBatch using the arrow-json Decoder.
fn decode_events(
    mut decoder: Decoder,
    events: &[Event],
) -> Result<RecordBatch, ArrowEncodingError> {
    let values: Vec<&vrl::value::Value> = events
        .iter()
        .filter_map(Event::maybe_as_log)
        .map(|log| log.value())
        .collect();

    decoder
        .serialize(&values)
        .map_err(|source| ArrowEncodingError::ArrowJsonDecode { source })?;

    decoder
        .flush()
        .map_err(|source| ArrowEncodingError::ArrowJsonDecode { source })?
        .ok_or(ArrowEncodingError::NoEvents)
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
        datatypes::TimeUnit,
        ipc::reader::StreamReader,
    };
    use chrono::Utc;
    use std::io::Cursor;
    use vector_core::event::{LogEvent, Value};

    /// Helper to encode events and return the decoded RecordBatch
    fn encode_and_decode(
        events: Vec<Event>,
        schema: SchemaRef,
    ) -> Result<RecordBatch, Box<dyn std::error::Error>> {
        let bytes = encode_events_to_arrow_ipc_stream(&events, Some(Arc::clone(&schema)))?;
        let cursor = Cursor::new(bytes);
        let mut reader = StreamReader::try_new(cursor, None)?;
        Ok(reader.next().unwrap()?)
    }

    /// Create a simple event from key-value pairs
    fn create_event<V>(fields: Vec<(&str, V)>) -> Event
    where
        V: Into<Value>,
    {
        let mut log = LogEvent::default();
        for (key, value) in fields {
            log.insert(key, value.into());
        }
        Event::Log(log)
    }

    /// Assert a primitive value at a specific column and row
    macro_rules! assert_primitive_value {
        ($batch:expr, $col:expr, $row:expr, $array_type:ty, $expected:expr) => {
            assert_eq!(
                $batch
                    .column($col)
                    .as_any()
                    .downcast_ref::<$array_type>()
                    .unwrap()
                    .value($row),
                $expected
            )
        };
    }

    mod comprehensive {
        use super::*;

        #[test]
        fn test_encode_all_types() {
            use arrow::array::{
                Decimal128Array, ListArray, MapArray, UInt8Array, UInt16Array, UInt32Array,
                UInt64Array,
            };
            use vrl::value::ObjectMap;

            let now = Utc::now();

            // Create a struct (tuple) value with unnamed fields
            let mut tuple_value = ObjectMap::new();
            tuple_value.insert("f0".into(), Value::Bytes("nested_str".into()));
            tuple_value.insert("f1".into(), Value::Integer(999));

            // Create a named struct (named tuple) value
            let mut named_tuple_value = ObjectMap::new();
            named_tuple_value.insert("category".into(), Value::Bytes("test_category".into()));
            named_tuple_value.insert("tag".into(), Value::Bytes("test_tag".into()));

            // Create a list value
            let list_value = Value::Array(vec![
                Value::Integer(1),
                Value::Integer(2),
                Value::Integer(3),
            ]);

            // Create a map value
            let mut map_value = ObjectMap::new();
            map_value.insert("key1".into(), Value::Integer(100));
            map_value.insert("key2".into(), Value::Integer(200));

            let mut log = LogEvent::default();
            // Primitive types
            log.insert("string_field", "test");
            log.insert("int8_field", 127);
            log.insert("int16_field", 32000);
            log.insert("int32_field", 1000000);
            log.insert("int64_field", 42);
            log.insert("uint8_field", 255);
            log.insert("uint16_field", 65535);
            log.insert("uint32_field", 4000000);
            log.insert("uint64_field", 9000000000_i64);
            log.insert("float32_field", 3.15);
            log.insert("float64_field", 3.15);
            log.insert("bool_field", true);
            log.insert("bytes_field", bytes::Bytes::from("binary"));
            log.insert("timestamp_field", now);
            log.insert("decimal_field", 99.99);
            // Complex types
            log.insert("list_field", list_value);
            log.insert("struct_field", Value::Object(tuple_value));
            log.insert("named_struct_field", Value::Object(named_tuple_value));
            log.insert("map_field", Value::Object(map_value));

            let events = vec![Event::Log(log)];

            // Build schema with all supported types
            let struct_fields = arrow::datatypes::Fields::from(vec![
                Field::new("f0", DataType::Utf8, true),
                Field::new("f1", DataType::Int64, true),
            ]);

            let named_struct_fields = arrow::datatypes::Fields::from(vec![
                Field::new("category", DataType::Utf8, true),
                Field::new("tag", DataType::Utf8, true),
            ]);

            let map_entries = Field::new(
                "entries",
                DataType::Struct(arrow::datatypes::Fields::from(vec![
                    Field::new("keys", DataType::Utf8, false),
                    Field::new("values", DataType::Int64, true),
                ])),
                false,
            );

            let schema = SchemaRef::new(Schema::new(vec![
                Field::new("string_field", DataType::Utf8, true),
                Field::new("int8_field", DataType::Int8, true),
                Field::new("int16_field", DataType::Int16, true),
                Field::new("int32_field", DataType::Int32, true),
                Field::new("int64_field", DataType::Int64, true),
                Field::new("uint8_field", DataType::UInt8, true),
                Field::new("uint16_field", DataType::UInt16, true),
                Field::new("uint32_field", DataType::UInt32, true),
                Field::new("uint64_field", DataType::UInt64, true),
                Field::new("float32_field", DataType::Float32, true),
                Field::new("float64_field", DataType::Float64, true),
                Field::new("bool_field", DataType::Boolean, true),
                Field::new("bytes_field", DataType::Binary, true),
                Field::new(
                    "timestamp_field",
                    DataType::Timestamp(TimeUnit::Millisecond, None),
                    true,
                ),
                Field::new("decimal_field", DataType::Decimal128(10, 2), true),
                Field::new(
                    "list_field",
                    DataType::List(Field::new("item", DataType::Int64, true).into()),
                    true,
                ),
                Field::new("struct_field", DataType::Struct(struct_fields), true),
                Field::new(
                    "named_struct_field",
                    DataType::Struct(named_struct_fields),
                    true,
                ),
                Field::new("map_field", DataType::Map(map_entries.into(), false), true),
            ]));

            let batch = encode_and_decode(events, schema).expect("Failed to encode");

            assert_eq!(batch.num_rows(), 1);
            assert_eq!(batch.num_columns(), 19);

            // Verify all primitive types
            assert_eq!(
                batch
                    .column(0)
                    .as_any()
                    .downcast_ref::<StringArray>()
                    .unwrap()
                    .value(0),
                "test"
            );
            assert_primitive_value!(batch, 1, 0, arrow::array::Int8Array, 127);
            assert_primitive_value!(batch, 2, 0, arrow::array::Int16Array, 32000);
            assert_primitive_value!(batch, 3, 0, arrow::array::Int32Array, 1000000);
            assert_primitive_value!(batch, 4, 0, Int64Array, 42);
            assert_primitive_value!(batch, 5, 0, UInt8Array, 255);
            assert_primitive_value!(batch, 6, 0, UInt16Array, 65535);
            assert_primitive_value!(batch, 7, 0, UInt32Array, 4000000);
            assert_primitive_value!(batch, 8, 0, UInt64Array, 9000000000);
            assert!(
                (batch
                    .column(9)
                    .as_any()
                    .downcast_ref::<arrow::array::Float32Array>()
                    .unwrap()
                    .value(0)
                    - 3.15)
                    .abs()
                    < 0.001
            );
            assert!(
                (batch
                    .column(10)
                    .as_any()
                    .downcast_ref::<Float64Array>()
                    .unwrap()
                    .value(0)
                    - 3.15)
                    .abs()
                    < 0.001
            );
            assert!(
                batch
                    .column(11)
                    .as_any()
                    .downcast_ref::<BooleanArray>()
                    .unwrap()
                    .value(0)
            );
            assert_primitive_value!(batch, 12, 0, BinaryArray, b"binary");
            assert_primitive_value!(
                batch,
                13,
                0,
                TimestampMillisecondArray,
                now.timestamp_millis()
            );
            assert_primitive_value!(batch, 14, 0, Decimal128Array, 9999);

            let list_array = batch
                .column(15)
                .as_any()
                .downcast_ref::<ListArray>()
                .unwrap();
            assert!(!list_array.is_null(0));
            let list_value = list_array.value(0);
            assert_eq!(list_value.len(), 3);
            let int_array = list_value.as_any().downcast_ref::<Int64Array>().unwrap();
            assert_eq!(int_array.value(0), 1);
            assert_eq!(int_array.value(1), 2);
            assert_eq!(int_array.value(2), 3);

            // Verify struct field (unnamed)
            let struct_array = batch
                .column(16)
                .as_any()
                .downcast_ref::<arrow::array::StructArray>()
                .unwrap();
            assert!(!struct_array.is_null(0));
            assert_primitive_value!(struct_array, 0, 0, StringArray, "nested_str");
            assert_primitive_value!(struct_array, 1, 0, Int64Array, 999);

            // Verify named struct field (named tuple)
            let named_struct_array = batch
                .column(17)
                .as_any()
                .downcast_ref::<arrow::array::StructArray>()
                .unwrap();
            assert!(!named_struct_array.is_null(0));
            assert_primitive_value!(named_struct_array, 0, 0, StringArray, "test_category");
            assert_primitive_value!(named_struct_array, 1, 0, StringArray, "test_tag");

            // Verify map field
            let map_array = batch
                .column(18)
                .as_any()
                .downcast_ref::<MapArray>()
                .unwrap();
            assert!(!map_array.is_null(0));
            let map_value = map_array.value(0);
            assert_eq!(map_value.len(), 2);
        }
    }

    mod error_handling {
        use super::*;

        #[test]
        fn test_encode_without_schema_fails() {
            let events = vec![create_event(vec![("message", "hello")])];

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
        fn test_null_constraint_error() {
            let events = vec![create_event(vec![("other_field", "value")])];

            let schema = SchemaRef::new(Schema::new(vec![Field::new(
                "required_field",
                DataType::Utf8,
                false, // non-nullable
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
    }

    mod temporal_types {
        use super::*;

        #[test]
        fn test_encode_timestamp_precisions() {
            let now = Utc::now();
            let mut log = LogEvent::default();
            log.insert("ts_second", now);
            log.insert("ts_milli", now);
            log.insert("ts_micro", now);
            log.insert("ts_nano", now);

            let events = vec![Event::Log(log)];

            let schema = SchemaRef::new(Schema::new(vec![
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

            let batch = encode_and_decode(events, schema).unwrap();

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
        fn test_encode_mixed_timestamp_string_native_and_integer() {
            let now = Utc::now();

            let mut log1 = LogEvent::default();
            log1.insert("ts", "2025-10-22T10:18:44.256Z"); // RFC3339 String

            let mut log2 = LogEvent::default();
            log2.insert("ts", now); // Native Timestamp

            let mut log3 = LogEvent::default();
            log3.insert("ts", 1729594724256000000_i64); // Integer (nanoseconds)

            let events = vec![Event::Log(log1), Event::Log(log2), Event::Log(log3)];

            let schema = SchemaRef::new(Schema::new(vec![Field::new(
                "ts",
                DataType::Timestamp(TimeUnit::Nanosecond, Some("+00:00".into())),
                true,
            )]));

            let batch = encode_and_decode(events, schema).unwrap();

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

            // First one should match the parsed RFC3339 string
            let expected = chrono::DateTime::parse_from_rfc3339("2025-10-22T10:18:44.256Z")
                .unwrap()
                .timestamp_nanos_opt()
                .unwrap();
            assert_eq!(ts_array.value(0), expected);

            // Second one should match the native timestamp
            assert_eq!(ts_array.value(1), now.timestamp_nanos_opt().unwrap());

            // Third one should match the integer
            assert_eq!(ts_array.value(2), 1729594724256000000_i64);
        }
    }

    mod config_tests {
        use super::*;
        use tokio_util::codec::Encoder;

        #[test]
        fn test_config_allow_nullable_fields_overrides_schema() {
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
            let inner_struct_field = Field::new("nested_field", DataType::Int64, false);
            let inner_struct =
                DataType::Struct(arrow::datatypes::Fields::from(vec![inner_struct_field]));
            let list_field = Field::new("item", inner_struct, false);
            let list_type = DataType::List(list_field.into());
            let outer_field = Field::new("inner_list", list_type, false);
            let outer_struct = DataType::Struct(arrow::datatypes::Fields::from(vec![outer_field]));

            let original_field = Field::new("root", outer_struct, false);
            let nullable_field = make_field_nullable(&original_field).unwrap();

            assert!(
                nullable_field.is_nullable(),
                "Root field should be nullable"
            );

            if let DataType::Struct(root_fields) = nullable_field.data_type() {
                let inner_list_field = &root_fields[0];
                assert!(inner_list_field.is_nullable());

                if let DataType::List(list_item_field) = inner_list_field.data_type() {
                    assert!(list_item_field.is_nullable());

                    if let DataType::Struct(inner_struct_fields) = list_item_field.data_type() {
                        let nested_field = &inner_struct_fields[0];
                        assert!(nested_field.is_nullable());
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
            let key_field = Field::new("key", DataType::Utf8, false);
            let value_field = Field::new("value", DataType::Int64, false);
            let entries_struct =
                DataType::Struct(arrow::datatypes::Fields::from(vec![key_field, value_field]));
            let entries_field = Field::new("entries", entries_struct, false);
            let map_type = DataType::Map(entries_field.into(), false);

            let original_field = Field::new("my_map", map_type, false);
            let nullable_field = make_field_nullable(&original_field).unwrap();

            assert!(
                nullable_field.is_nullable(),
                "Root map field should be nullable"
            );

            if let DataType::Map(entries_field, _sorted) = nullable_field.data_type() {
                assert!(
                    !entries_field.is_nullable(),
                    "Map entries field should be non-nullable"
                );

                if let DataType::Struct(struct_fields) = entries_field.data_type() {
                    let key_field = &struct_fields[0];
                    let value_field = &struct_fields[1];
                    assert!(
                        !key_field.is_nullable(),
                        "Map key field should be non-nullable"
                    );
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
}
