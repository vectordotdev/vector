use crate::encoding::BuildError;
use bytes::BytesMut;
use prost_reflect::{prost::Message as _, MessageDescriptor};
use std::path::PathBuf;
use tokio_util::codec::Encoder;
use vector_core::{
    config::DataType,
    event::{Event, Value},
    schema,
};

/// Config used to build a `ProtobufSerializer`.
#[crate::configurable_component]
#[derive(Debug, Clone)]
pub struct ProtobufSerializerConfig {
    /// Options for the Protobuf serializer.
    pub protobuf: ProtobufSerializerOptions,
}

impl ProtobufSerializerConfig {
    /// Build the `ProtobufSerializer` from this configuration.
    pub fn build(&self) -> Result<ProtobufSerializer, BuildError> {
        let message_descriptor = vrl::protobuf::get_message_descriptor(
            &self.protobuf.desc_file,
            &self.protobuf.message_type,
        )?;
        Ok(ProtobufSerializer { message_descriptor })
    }

    /// The data type of events that are accepted by `ProtobufSerializer`.
    pub fn input_type(&self) -> DataType {
        DataType::Log
    }

    /// The schema required by the serializer.
    pub fn schema_requirement(&self) -> schema::Requirement {
        // While technically we support `Value` variants that can't be losslessly serialized to
        // Protobuf, we don't want to enforce that limitation to users yet.
        schema::Requirement::empty()
    }
}

/// Protobuf serializer options.
#[crate::configurable_component]
#[derive(Debug, Clone)]
pub struct ProtobufSerializerOptions {
    /// The path to the protobuf descriptor set file.
    ///
    /// This file is the output of `protoc -o <path> ...`
    #[configurable(metadata(docs::examples = "/etc/vector/protobuf_descriptor_set.desc"))]
    pub desc_file: PathBuf,

    /// The name of the message type to use for serializing.
    #[configurable(metadata(docs::examples = "package.Message"))]
    pub message_type: String,
}

/// Serializer that converts an `Event` to bytes using the Protobuf format.
#[derive(Debug, Clone)]
pub struct ProtobufSerializer {
    /// The protobuf message definition to use for serialization.
    message_descriptor: MessageDescriptor,
}

impl ProtobufSerializer {
    /// Creates a new `ProtobufSerializer`.
    pub fn new(message_descriptor: MessageDescriptor) -> Self {
        Self { message_descriptor }
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
                vrl::protobuf::encode_message(&self.message_descriptor, log.into_parts().0)
            }
            Event::Metric(_) => unimplemented!(),
            Event::Trace(trace) => vrl::protobuf::encode_message(
                &self.message_descriptor,
                Value::Object(trace.into_parts().0),
            ),
        }?;
        message.encode(buffer).map_err(Into::into)
    }
}
