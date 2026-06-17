use bytes::BytesMut;
use prost::Message;
use serde::{Deserialize, Serialize};
use tokio_util::codec::Encoder;
use vector_common::internal_event::{ComponentEventsDropped, UNINTENTIONAL, emit};
use vector_core::{
    config::DataType,
    event::{Event, EventArray, EventStatus, event_exceeds_max_nesting_cost, proto},
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
        if let Some((cost, budget)) = event_exceeds_max_nesting_cost(&event) {
            // Returning `Err` here would propagate through batched encoders
            // (e.g. `src/sinks/util/encoding.rs`) as a fatal `InvalidData`,
            // causing the entire request to be dropped and every other valid
            // event in the batch to be lost. Instead drop just this event with
            // proper telemetry and `EventStatus::Rejected`, write zero bytes,
            // and return Ok so batched callers continue to the next event.
            //
            // Per-event callers (e.g. `WriterSink::run`) treat `Ok(())` with an
            // empty buffer as an in-encoder drop and finalize accordingly.
            let reason = format!("event nesting cost ({cost}) exceeds protobuf budget ({budget})");
            emit(ComponentEventsDropped::<UNINTENTIONAL> {
                count: 1,
                reason: &reason,
            });
            match event {
                Event::Log(log) => log.metadata().update_status(EventStatus::Rejected),
                Event::Metric(metric) => metric.metadata().update_status(EventStatus::Rejected),
                Event::Trace(trace) => trace.metadata().update_status(EventStatus::Rejected),
            }
            return Ok(());
        }
        let array = EventArray::from(event);
        let proto = proto::EventArray::from(array);
        proto.encode(buffer)?;
        Ok(())
    }
}
