//! Protobuf batch serializer for encoding events as individual protobuf records.
//!
//! Encodes each event in a batch independently into protobuf bytes, producing
//! a `Vec<Vec<u8>>` where each element is a single serialized protobuf message.

use prost_reflect::{MessageDescriptor, prost::Message as _};
use snafu::Snafu;
use std::sync::Arc;
use vector_config::configurable_component;
use vector_core::{
    config::DataType,
    event::Event,
    schema,
};
use vrl::protobuf::encode::{Options, encode_message};

/// Errors that can occur during protobuf batch encoding
#[derive(Debug, Snafu)]
pub enum ProtoBatchEncodingError {
    /// No events provided
    #[snafu(display("Cannot encode an empty batch"))]
    NoEvents,

    /// Unsupported event type
    #[snafu(display("Unsupported event type: only Log and Trace events are supported"))]
    UnsupportedEventType,

    /// Protobuf encoding failed
    #[snafu(display("Protobuf encoding failed: {}", source))]
    EncodingFailed {
        /// The underlying encoding error
        source: vector_common::Error,
    },

    /// Protobuf prost encoding failed
    #[snafu(display("Protobuf prost encoding failed: {}", source))]
    ProstEncodingFailed {
        /// The underlying prost error
        source: prost_reflect::prost::EncodeError,
    },
}

/// Configuration for protobuf batch serialization
#[configurable_component]
#[derive(Clone, Default)]
pub struct ProtoBatchSerializerConfig {
    /// The protobuf message descriptor to use for encoding.
    #[serde(skip)]
    #[configurable(derived)]
    pub descriptor: Option<MessageDescriptor>,
}

impl std::fmt::Debug for ProtoBatchSerializerConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProtoBatchSerializerConfig")
            .field(
                "descriptor",
                &self.descriptor.as_ref().map(|d| d.full_name().to_string()),
            )
            .finish()
    }
}

impl ProtoBatchSerializerConfig {
    /// Create a new ProtoBatchSerializerConfig with a message descriptor
    pub fn new(descriptor: MessageDescriptor) -> Self {
        Self {
            descriptor: Some(descriptor),
        }
    }

    /// The data type of events that are accepted by this serializer.
    pub fn input_type(&self) -> DataType {
        DataType::Log | DataType::Trace
    }

    /// The schema required by the serializer.
    pub fn schema_requirement(&self) -> schema::Requirement {
        schema::Requirement::empty()
    }
}

/// Protobuf batch serializer that encodes each event into individual protobuf bytes.
#[derive(Clone, Debug)]
pub struct ProtoBatchSerializer {
    descriptor: Arc<MessageDescriptor>,
    options: Options,
}

impl ProtoBatchSerializer {
    /// Create a new ProtoBatchSerializer with the given configuration.
    pub fn new(config: ProtoBatchSerializerConfig) -> Result<Self, vector_common::Error> {
        let descriptor = config.descriptor.ok_or_else(|| {
            vector_common::Error::from("Proto batch serializer requires a message descriptor.")
        })?;

        Ok(Self {
            descriptor: Arc::new(descriptor),
            options: Options {
                use_json_names: false,
            },
        })
    }

    /// Encode a batch of events into individual protobuf byte buffers.
    pub fn encode_batch(
        &self,
        events: &[Event],
    ) -> Result<Vec<Vec<u8>>, ProtoBatchEncodingError> {
        if events.is_empty() {
            return Err(ProtoBatchEncodingError::NoEvents);
        }

        let mut records = Vec::with_capacity(events.len());

        for event in events {
            let dynamic_message = match event {
                Event::Log(log) => {
                    encode_message(&self.descriptor, log.value().clone(), &self.options)
                }
                Event::Trace(_) | Event::Metric(_) => {
                    return Err(ProtoBatchEncodingError::UnsupportedEventType);
                }
            }
            .map_err(|source| ProtoBatchEncodingError::EncodingFailed { source: source.into() })?;

            records.push(dynamic_message.encode_to_vec());
        }

        Ok(records)
    }
}
