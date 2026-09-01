use std::sync::LazyLock;

use prost_reflect::{DescriptorPool, DynamicMessage, MessageDescriptor, ReflectMessage, Value};
use vector_core::event::{Event, proto};

static EVENT_WRAPPER_DESCRIPTOR: LazyLock<MessageDescriptor> = LazyLock::new(|| {
    let pool = match DescriptorPool::decode(proto::FILE_DESCRIPTOR_SET) {
        Ok(pool) => pool,
        Err(error) => panic!("native event descriptors must be valid: {error}"),
    };
    match pool.get_message_by_name("event.EventWrapper") {
        Some(descriptor) => descriptor,
        None => panic!("native event descriptors must contain event.EventWrapper"),
    }
});

pub(crate) fn to_json_value(event: Event) -> vector_common::Result<serde_json::Value> {
    let event = proto::EventWrapper::from(event);

    // The generated event types and prost-reflect currently use different prost
    // versions, so bridge between them through their shared wire representation.
    let mut dynamic = DynamicMessage::decode(
        EVENT_WRAPPER_DESCRIPTOR.clone(),
        prost::Message::encode_to_vec(&event).as_slice(),
    )
    .map_err(|error| format!("Error encoding native JSON event: {error}"))?;
    omit_deprecated_fields(&mut dynamic);
    let mut json = serde_json::to_value(dynamic)?;
    json.sort_all_objects();
    Ok(json)
}

pub(crate) fn from_dynamic_message(message: DynamicMessage) -> vector_common::Result<Event> {
    // See the corresponding wire bridge in `to_json_value`.
    let bytes = prost_reflect::prost::Message::encode_to_vec(&message);
    let event = <proto::EventWrapper as prost::Message>::decode(bytes.as_slice())
        .map_err(|error| format!("Error decoding native JSON event: {error}"))?;
    Ok(event.into())
}

pub(crate) fn descriptor() -> MessageDescriptor {
    EVENT_WRAPPER_DESCRIPTOR.clone()
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
