use crate::internal_events::PulsarPropertyExtractionError;
use crate::sinks::pulsar::config::PulsarSinkConfig;
use crate::sinks::pulsar::sink::PulsarEvent;
use crate::template::Template;
use bytes::Bytes;
use std::collections::HashMap;
use vector_lib::event::Event;
use vector_lib::lookup::lookup_v2::OptionalTargetPath;
use vrl::value::{KeyString, Value};

/// Transforms an event into a Pulsar event by rendering the required template fields.
/// Returns None if there is an error whilst rendering.
pub(super) fn make_pulsar_event(
    topic: &Template,
    config: &PulsarSinkConfig,
    event: Event,
) -> Option<PulsarEvent> {
    let topic = topic.render_string(&event).ok()?;
    let key = get_key(&event, &config.partition_key_field);
    let timestamp_millis = get_timestamp_millis(&event);
    let properties = get_properties(&event, &config.properties_key);
    Some(PulsarEvent {
        event,
        topic,
        key,
        timestamp_millis,
        properties,
    })
}

fn get_key(event: &Event, partition_key_field: &Option<OptionalTargetPath>) -> Option<Bytes> {
    partition_key_field
        .as_ref()
        .and_then(|partition_key_field| match event {
            Event::Log(log) => partition_key_field
                .path
                .as_ref()
                .and_then(|path| log.get(path).map(|value| value.coerce_to_bytes())),
            Event::Metric(metric) => partition_key_field
                .path
                .as_ref()
                .and_then(|path| metric.tags().and_then(|tags| tags.get(&path.to_string())))
                .map(|value| value.to_owned().into()),
            _ => None,
        })
}

fn get_timestamp_millis(event: &Event) -> Option<i64> {
    match &event {
        Event::Log(log) => log.get_timestamp().and_then(|v| v.as_timestamp()).copied(),
        Event::Metric(metric) => metric.timestamp(),
        _ => None,
    }
    .map(|ts| ts.timestamp_millis())
}

pub(super) fn get_properties(
    event: &Event,
    properties_key: &Option<OptionalTargetPath>,
) -> Option<HashMap<KeyString, Bytes>> {
    properties_key.as_ref().and_then(|properties_key| {
        properties_key.path.as_ref().and_then(|path| {
            event.maybe_as_log().and_then(|log| {
                log.get(path).and_then(|properties| match properties {
                    Value::Object(headers_map) => {
                        let mut property_map = HashMap::new();
                        for (key, value) in headers_map {
                            if let Value::Bytes(value_bytes) = value {
                                property_map.insert(key.clone(), value_bytes.clone());
                            } else {
                                emit!(PulsarPropertyExtractionError {
                                    property_field: path
                                });
                            }
                        }
                        Some(property_map)
                    }
                    _ => {
                        emit!(PulsarPropertyExtractionError {
                            property_field: path
                        });
                        None
                    }
                })
            })
        })
    })
}
