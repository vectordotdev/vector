use std::path::PathBuf;

use crate::encoding::BuildError;
use bytes::BytesMut;
use prost_reflect::{MessageDescriptor, prost::Message as _};
use tokio_util::codec::Encoder;
use vector_config_macros::configurable_component;
use vector_core::{
    config::DataType,
    event::{Event, Value},
    schema,
};
use vrl::protobuf::{
    descriptor::{get_message_descriptor, get_message_descriptor_from_bytes},
    encode::{Options, encode_message},
};

/// Config used to build a `ProtobufSerializer`.
#[configurable_component]
#[derive(Debug, Clone)]
pub struct ProtobufSerializerConfig {
    /// Options for the Protobuf serializer.
    pub protobuf: ProtobufSerializerOptions,
}

impl ProtobufSerializerConfig {
    /// Build the `ProtobufSerializer` from this configuration.
    pub fn build(&self) -> Result<ProtobufSerializer, BuildError> {
        let message_descriptor =
            get_message_descriptor(&self.protobuf.desc_file, &self.protobuf.message_type)?;
        Ok(ProtobufSerializer {
            message_descriptor,
            options: Options {
                use_json_names: self.protobuf.use_json_names,
            },
        })
    }

    /// The data type of events that are accepted by `ProtobufSerializer`.
    pub fn input_type(&self) -> DataType {
        DataType::Log | DataType::Trace
    }

    /// The schema required by the serializer.
    pub fn schema_requirement(&self) -> schema::Requirement {
        // While technically we support `Value` variants that can't be losslessly serialized to
        // Protobuf, we don't want to enforce that limitation to users yet.
        schema::Requirement::empty()
    }
}

/// Protobuf serializer options.
#[configurable_component]
#[derive(Debug, Clone)]
pub struct ProtobufSerializerOptions {
    /// The path to the protobuf descriptor set file.
    ///
    /// This file is the output of `protoc -I <include path> -o <desc output path> <proto>`
    ///
    /// You can read more [here](https://buf.build/docs/reference/images/#how-buf-images-work).
    #[configurable(metadata(docs::examples = "/etc/vector/protobuf_descriptor_set.desc"))]
    pub desc_file: PathBuf,

    /// The name of the message type to use for serializing.
    #[configurable(metadata(docs::examples = "package.Message"))]
    pub message_type: String,

    /// Use JSON field names (camelCase) instead of protobuf field names (snake_case).
    ///
    /// When enabled, the serializer looks for fields using their JSON names as defined
    /// in the `.proto` file (for example `jobDescription` instead of `job_description`).
    ///
    /// This is useful when working with data that has already been converted from JSON or
    /// when interfacing with systems that use JSON naming conventions.
    #[serde(default, skip_serializing_if = "vector_core::serde::is_default")]
    pub use_json_names: bool,
}

/// Serializer that converts an `Event` to bytes using the Protobuf format.
#[derive(Debug, Clone)]
pub struct ProtobufSerializer {
    /// The protobuf message definition to use for serialization.
    message_descriptor: MessageDescriptor,
    options: Options,
}

impl ProtobufSerializer {
    /// Creates a new `ProtobufSerializer`.
    pub fn new(message_descriptor: MessageDescriptor) -> Self {
        Self {
            message_descriptor,
            options: Options::default(),
        }
    }

    /// Creates a new serializer instance using the descriptor bytes directly.
    pub fn new_from_bytes(
        desc_bytes: &[u8],
        message_type: &str,
        options: &Options,
    ) -> vector_common::Result<Self> {
        let message_descriptor = get_message_descriptor_from_bytes(desc_bytes, message_type)?;
        Ok(Self {
            message_descriptor,
            options: options.clone(),
        })
    }

    /// Get a description of the message type used in serialization.
    pub fn descriptor_proto(&self) -> &prost_reflect::prost_types::DescriptorProto {
        self.message_descriptor.descriptor_proto()
    }
}

impl Encoder<Event> for ProtobufSerializer {
    type Error = vector_common::Error;

    fn encode(&mut self, event: Event, buffer: &mut BytesMut) -> Result<(), Self::Error> {
        let message = match event {
            Event::Log(log) => {
                encode_message(&self.message_descriptor, log.into_parts().0, &self.options)
            }
            Event::Metric(_) => unimplemented!(),
            Event::Trace(trace) => encode_message(
                &self.message_descriptor,
                Value::Object(trace.into_parts().0),
                &self.options,
            ),
        }?;
        message.encode(buffer).map_err(Into::into)
    }
}
