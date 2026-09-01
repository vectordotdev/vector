use std::sync::LazyLock;

use chrono::{TimeZone, Utc};
use prost_reflect::{DescriptorPool, DynamicMessage, MessageDescriptor, ReflectMessage, Value};
use vector_core::event::{Event, proto};

static DESCRIPTOR_POOL: LazyLock<DescriptorPool> =
    LazyLock::new(
        || match DescriptorPool::decode(proto::FILE_DESCRIPTOR_SET) {
            Ok(pool) => pool,
            Err(error) => panic!("native event descriptors must be valid: {error}"),
        },
    );

static EVENT_WRAPPER_DESCRIPTOR: LazyLock<MessageDescriptor> =
    LazyLock::new(
        || match DESCRIPTOR_POOL.get_message_by_name("event.EventWrapper") {
            Some(descriptor) => descriptor,
            None => panic!("native event descriptors must contain event.EventWrapper"),
        },
    );

static NATIVE_JSON_DESCRIPTOR: LazyLock<MessageDescriptor> = LazyLock::new(
    || match DESCRIPTOR_POOL.get_message_by_name("event.NativeJsonEnvelope") {
        Some(descriptor) => descriptor,
        None => panic!("native event descriptors must contain event.NativeJsonEnvelope"),
    },
);

pub(crate) fn to_json_value(event: Event) -> vector_common::Result<serde_json::Value> {
    let event = proto::EventWrapper::from_event_for_native_json(event);

    // The generated event types and prost-reflect currently use different prost
    // versions, so bridge between them through their shared wire representation.
    let mut dynamic = DynamicMessage::decode(
        EVENT_WRAPPER_DESCRIPTOR.clone(),
        prost::Message::encode_to_vec(&event).as_slice(),
    )
    .map_err(|error| format!("Error encoding native JSON event: {error}"))?;
    omit_deprecated_fields(&mut dynamic);

    let event_field = NATIVE_JSON_DESCRIPTOR
        .get_field_by_name("event")
        .unwrap_or_else(|| panic!("event.NativeJsonEnvelope must contain an event field"));
    let mut envelope = DynamicMessage::new(NATIVE_JSON_DESCRIPTOR.clone());
    envelope.set_field(&event_field, Value::Message(dynamic));

    let mut json = serde_json::to_value(envelope)?;
    json.sort_all_objects();
    Ok(json)
}

pub(crate) fn from_dynamic_message(message: DynamicMessage) -> vector_common::Result<Event> {
    let event_field = NATIVE_JSON_DESCRIPTOR
        .get_field_by_name("event")
        .unwrap_or_else(|| panic!("event.NativeJsonEnvelope must contain an event field"));
    let event_value = message.get_field(&event_field);
    let event = match event_value.as_ref() {
        Value::Message(event) if message.has_field(&event_field) => event,
        _ => return Err("Error decoding native JSON event: envelope is missing event".into()),
    };

    // See the corresponding wire bridge in `to_json_value`.
    let bytes = prost_reflect::prost::Message::encode_to_vec(event);
    let event = <proto::EventWrapper as prost::Message>::decode(bytes.as_slice())
        .map_err(|error| format!("Error decoding native JSON event: {error}"))?;
    validate_event_wrapper(&event)
        .map_err(|error| format!("Error decoding native JSON event: {error}"))?;
    Ok(event.into())
}

pub(crate) fn descriptor() -> MessageDescriptor {
    NATIVE_JSON_DESCRIPTOR.clone()
}

#[allow(deprecated)]
fn validate_event_wrapper(wrapper: &proto::EventWrapper) -> Result<(), &'static str> {
    let event = wrapper.event.as_ref().ok_or("wrapper is missing event")?;

    match event {
        proto::event_wrapper::Event::Log(log) => {
            validate_value(log.value.as_ref().ok_or("log is missing value")?)?;
            validate_optional_value(log.metadata.as_ref())?;
            validate_metadata(log.metadata_full.as_ref())
        }
        proto::event_wrapper::Event::Trace(trace) => {
            validate_values(trace.fields.values())?;
            validate_optional_value(trace.metadata.as_ref())?;
            validate_metadata(trace.metadata_full.as_ref())
        }
        proto::event_wrapper::Event::Metric(metric) => {
            if let Some(timestamp) = &metric.timestamp {
                validate_timestamp(timestamp.seconds, timestamp.nanos)?;
            }
            let value = metric.value.as_ref().ok_or("metric is missing value")?;
            if let proto::metric::Value::Sketch(sketch) = value {
                let sketch = sketch.sketch.as_ref().ok_or("sketch is missing value")?;
                let proto::sketch::Sketch::AgentDdSketch(sketch) = sketch;
                if sketch.k.len() != sketch.n.len() {
                    return Err("sketch bin keys and counts have different lengths");
                }
            }
            validate_optional_value(metric.metadata.as_ref())?;
            validate_metadata(metric.metadata_full.as_ref())
        }
    }
}

fn validate_metadata(metadata: Option<&proto::Metadata>) -> Result<(), &'static str> {
    if let Some(metadata) = metadata {
        validate_optional_value(metadata.value.as_ref())?;
    }
    Ok(())
}

fn validate_optional_value(value: Option<&proto::Value>) -> Result<(), &'static str> {
    if let Some(value) = value {
        validate_value(value)?;
    }
    Ok(())
}

fn validate_values<'a>(
    values: impl IntoIterator<Item = &'a proto::Value>,
) -> Result<(), &'static str> {
    for value in values {
        validate_value(value)?;
    }
    Ok(())
}

fn validate_value(value: &proto::Value) -> Result<(), &'static str> {
    match value.kind.as_ref().ok_or("value is missing kind")? {
        proto::value::Kind::Timestamp(timestamp) => {
            validate_timestamp(timestamp.seconds, timestamp.nanos)
        }
        proto::value::Kind::Float(value) if value.is_nan() => Err("value contains NaN"),
        proto::value::Kind::Map(map) => validate_values(map.fields.values()),
        proto::value::Kind::Array(array) => validate_values(&array.items),
        _ => Ok(()),
    }
}

fn validate_timestamp(seconds: i64, nanos: i32) -> Result<(), &'static str> {
    #[allow(clippy::cast_sign_loss)]
    match Utc.timestamp_opt(seconds, nanos as u32).single() {
        Some(_) => Ok(()),
        None => Err("value contains an invalid timestamp"),
    }
}

fn omit_deprecated_fields(message: &mut DynamicMessage) {
    let fields = message.descriptor().fields().collect::<Vec<_>>();
    for field in fields {
        if field
            .field_descriptor_proto()
            .options
            .as_ref()
            .and_then(|options| options.deprecated)
            .unwrap_or(false)
        {
            message.clear_field(&field);
        } else if message.has_field(&field) {
            omit_deprecated_nested_fields(message.get_field_mut(&field));
        }
    }
}

fn omit_deprecated_nested_fields(value: &mut Value) {
    match value {
        Value::Message(message) => omit_deprecated_fields(message),
        Value::List(values) => values.iter_mut().for_each(omit_deprecated_nested_fields),
        Value::Map(values) => values.values_mut().for_each(omit_deprecated_nested_fields),
        _ => {}
    }
}
