//! Public-facing config + serializer types.
//!
//! [`WireToArrowSerializerConfig`] is the `BatchSerializerConfig` variant
//! callers wire into a sink; [`WireToArrowSerializer`] is the runtime
//! object built from a config that turns a batch of events into a
//! `RecordBatch` via an underlying [`WireToArrowEncoder`].

use std::path::PathBuf;
use std::sync::Arc;

use arrow::datatypes::Schema;
use arrow::record_batch::RecordBatch;
use prost_reflect::MessageDescriptor;
use vector_config::configurable_component;
use vector_core::{
    config::DataType,
    event::{Event, Value},
    schema,
};
use vrl::protobuf::descriptor::get_message_descriptor;

use super::encoder::WireToArrowEncoder;
use super::errors::{Result, WireToArrowError};

/// Configuration for the wire-to-Arrow batch serializer.
///
/// `desc_file` + `message_type` identify the proto descriptor for the
/// *incoming* wire bytes — the user must supply them directly, mirroring
/// [`ProtobufSerializerOptions`]. The sink injects the output Arrow `schema`
/// at build time (typically derived from its own schema source).
///
/// The descriptor must describe the exact bytes present in `event.message`;
/// decoding uses the descriptor's field numbers as-is. If the payload needs
/// any pre-processing before it matches the descriptor, do it upstream.
///
/// [`ProtobufSerializerOptions`]: crate::encoding::format::ProtobufSerializerOptions
#[configurable_component]
#[derive(Clone, Default)]
pub struct WireToArrowSerializerConfig {
    /// Path to the protobuf descriptor set file describing the incoming wire bytes.
    ///
    /// Must correspond to the exact proto type serialized in `event.message`.
    /// Typically the output of `protoc -I <include path> -o <desc output path> <proto>`.
    #[configurable(metadata(docs::examples = "/etc/vector/protobuf_descriptor_set.desc"))]
    pub desc_file: PathBuf,

    /// The fully-qualified message type within the descriptor file. Must name
    /// the type of the bytes in `event.message`.
    #[configurable(metadata(docs::examples = "package.Message"))]
    pub message_type: String,

    /// The Arrow schema of the output `RecordBatch`. Injected by the sink.
    #[serde(skip)]
    #[configurable(derived)]
    pub schema: Option<arrow::datatypes::Schema>,
}

impl std::fmt::Debug for WireToArrowSerializerConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WireToArrowSerializerConfig")
            .field("desc_file", &self.desc_file)
            .field("message_type", &self.message_type)
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

impl WireToArrowSerializerConfig {
    /// The data type of events accepted by this serializer.
    pub fn input_type(&self) -> DataType {
        DataType::Log
    }

    /// The schema required by the serializer.
    pub fn schema_requirement(&self) -> schema::Requirement {
        schema::Requirement::empty()
    }
}

/// Batch serializer that decodes proto wire bytes directly into an Arrow
/// `RecordBatch`, bypassing the generic `ProtobufDeserializer` chain.
#[derive(Clone, Debug)]
pub struct WireToArrowSerializer {
    encoder: Arc<WireToArrowEncoder>,
}

impl WireToArrowSerializer {
    /// Build a serializer from the given configuration. Loads the proto
    /// descriptor from `desc_file` + `message_type`; the output Arrow schema
    /// must have been injected (via `config.schema`) by the sink.
    pub fn new(config: WireToArrowSerializerConfig) -> Result<Self> {
        let descriptor = get_message_descriptor(&config.desc_file, &config.message_type)
            .map_err(|message| WireToArrowError::DescriptorLoad { message })?;
        let schema = config
            .schema
            .ok_or_else(|| WireToArrowError::ConfigurationMissing { field: "schema" })?;
        Self::from_descriptor(descriptor, schema)
    }

    /// Build a serializer from an already-resolved descriptor and schema.
    /// Mostly useful for tests and for callers that have the descriptor in
    /// memory already.
    pub fn from_descriptor(descriptor: MessageDescriptor, schema: Schema) -> Result<Self> {
        let encoder = WireToArrowEncoder::new(&descriptor, schema)?;
        Ok(Self {
            encoder: Arc::new(encoder),
        })
    }

    /// Encode a batch of events into a single Arrow `RecordBatch`.
    ///
    /// Every event must carry a `Value::Bytes`-typed `message` field holding
    /// the original proto wire bytes; any miss rejects the batch.
    pub fn encode_to_record_batch(&self, events: &[Event]) -> Result<RecordBatch> {
        if events.is_empty() {
            return Err(WireToArrowError::NoEvents);
        }
        let mut wire_bytes = Vec::with_capacity(events.len());
        for event in events {
            match event.as_log().get_message() {
                Some(Value::Bytes(b)) => wire_bytes.push(b.clone()),
                Some(_) => return Err(WireToArrowError::MessageBytesWrongType),
                None => return Err(WireToArrowError::MessageBytesMissing),
            }
        }
        self.encoder.encode_batch(&wire_bytes)
    }
}
