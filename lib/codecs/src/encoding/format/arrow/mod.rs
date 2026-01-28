//! Arrow IPC streaming format codec for batched event encoding
//!
//! Provides Apache Arrow IPC stream format encoding with static schema support.
//! This implements the streaming variant of the Arrow IPC protocol, which writes
//! a continuous stream of record batches without a file footer.

mod serializer;

#[cfg(test)]
mod _ignore_bench_test;
#[cfg(test)]
mod tests;

use arrow::{
    datatypes::{DataType, Fields, Schema, SchemaRef},
    ipc::writer::StreamWriter,
};
use async_trait::async_trait;
use bytes::{BufMut, Bytes, BytesMut};
use snafu::Snafu;
use std::sync::Arc;
use vector_config::configurable_component;

use serializer::build_record_batch;

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
            Schema::new_with_metadata(
                Fields::from_iter(schema.fields().iter().map(|f| make_field_nullable(f))),
                schema.metadata().clone(),
            )
        } else {
            schema
        };

        Ok(Self {
            schema: SchemaRef::new(schema),
        })
    }
}

impl tokio_util::codec::Encoder<Vec<vector_core::event::Event>> for ArrowStreamSerializer {
    type Error = ArrowEncodingError;

    fn encode(
        &mut self,
        events: Vec<vector_core::event::Event>,
        buffer: &mut BytesMut,
    ) -> Result<(), Self::Error> {
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

    /// Serde Arrow serialization error
    #[snafu(display("Serde Arrow error: {}", source))]
    SerdeArrow {
        /// The underlying serde_arrow error
        source: serde_arrow::Error,
    },

    /// Invalid timestamp value could not be parsed
    #[snafu(display("Invalid timestamp value for field '{}': {}", field_name, value))]
    InvalidTimestamp {
        /// The field name
        field_name: String,
        /// The invalid value
        value: String,
    },

    /// Invalid type for Decimal128 field
    #[snafu(display(
        "Invalid type for Decimal128 field '{}': expected Float, got {:?}",
        field_name,
        actual_type
    ))]
    InvalidDecimalType {
        /// The field name
        field_name: String,
        /// The actual type received
        actual_type: String,
    },

    /// Invalid Map structure in schema
    #[snafu(display(
        "Invalid Map structure for field '{}': expected 2 entry fields (key, value), got {}",
        field_name,
        num_fields
    ))]
    InvalidMapStructure {
        /// The field name
        field_name: String,
        /// The number of fields found
        num_fields: usize,
    },

    /// Timestamp value overflows the representable range
    #[snafu(display(
        "Timestamp overflow for field '{}': value '{}' cannot be represented as i64 nanoseconds",
        field_name,
        timestamp
    ))]
    TimestampOverflow {
        /// The field name
        field_name: String,
        /// The timestamp value that overflowed
        timestamp: String,
    },
}

impl From<std::io::Error> for ArrowEncodingError {
    fn from(error: std::io::Error) -> Self {
        Self::Io { source: error }
    }
}

/// Encodes a batch of events into Arrow IPC streaming format
pub fn encode_events_to_arrow_ipc_stream(
    events: &[vector_core::event::Event],
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
fn make_field_nullable(field: &arrow::datatypes::Field) -> arrow::datatypes::Field {
    let new_data_type = match field.data_type() {
        DataType::List(inner_field) => DataType::List(make_field_nullable(inner_field).into()),
        DataType::Struct(fields) => DataType::Struct(Fields::from_iter(
            fields.iter().map(|f| make_field_nullable(f)),
        )),
        DataType::Map(inner, sorted) => {
            // A Map's inner field is typically a "entries" Struct<Key, Value>
            let DataType::Struct(fields) = inner.data_type() else {
                // Fallback for invalid Map structures (preserves original)
                return field.clone().with_nullable(true);
            };

            let new_struct_fields = vec![fields[0].clone(), make_field_nullable(&fields[1]).into()];

            // Reconstruct the inner "entries" field
            // The inner field itself must be non-nullable (only the Map wrapper is nullable)
            let new_inner_field = inner
                .as_ref()
                .clone()
                .with_data_type(DataType::Struct(new_struct_fields.into()))
                .with_nullable(false);

            DataType::Map(new_inner_field.into(), *sorted)
        }
        other => other.clone(),
    };

    field
        .clone()
        .with_data_type(new_data_type)
        .with_nullable(true)
}
