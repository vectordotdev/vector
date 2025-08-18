use bytes::BytesMut;
use prost::Message;
use serde::{Deserialize, Serialize};
use tokio_util::codec::Encoder;
use vector_core::{
    config::DataType,
    event::{proto, Event, EventArray},
    schema,
};

/// Config used to build a `NativeSerializer`.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct NativeSerializerConfig;

impl NativeSerializerConfig {
    /// Build the `NativeSerializer` from this configuration.
    pub const fn build(&self) -> NativeSerializer {
        NativeSerializer
    }

    /// The data type of events that are accepted by `NativeSerializer`.
    pub fn input_type(&self) -> DataType {
        DataType::all_bits()
    }

    /// The schema required by the serializer.
    pub fn schema_requirement(&self) -> schema::Requirement {
        schema::Requirement::empty()
    }
}

/// Serializer that converts an `Event` to bytes using the Vector native protobuf format.
#[derive(Debug, Clone)]
pub struct NativeSerializer;

impl Encoder<Event> for NativeSerializer {
    type Error = vector_common::Error;

    fn encode(&mut self, event: Event, buffer: &mut BytesMut) -> Result<(), Self::Error> {
        let array = EventArray::from(event);
        let proto = proto::EventArray::from(array);
        proto.encode(buffer)?;
        Ok(())
    }
}
