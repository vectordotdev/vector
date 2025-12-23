//! Arrow IPC streaming format codec for batched event encoding
//!
//! Provides Apache Arrow IPC stream format encoding with static schema support.
//! This implements the streaming variant of the Arrow IPC protocol, which writes
//! a continuous stream of record batches without a file footer.

mod builder;
mod types;

#[cfg(test)]
mod tests;

use arrow::{
    datatypes::{DataType, Schema},
    ipc::writer::StreamWriter,
};
use async_trait::async_trait;
use bytes::{BufMut, Bytes, BytesMut};
use snafu::Snafu;
use std::sync::Arc;
use vector_config::configurable_component;

use builder::build_record_batch;

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
}

impl From<std::io::Error> for ArrowEncodingError {
    fn from(error: std::io::Error) -> Self {
        Self::Io { source: error }
    }
}

/// Encodes a batch of events into Arrow IPC streaming format
pub fn encode_events_to_arrow_ipc_stream(
    events: &[vector_core::event::Event],
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
pub(crate) fn make_field_nullable(field: &arrow::datatypes::Field) -> arrow::datatypes::Field {
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
