use std::collections::HashMap;

use bytes::Bytes;
use vector_lib::{event::Event, lookup::lookup_v2::OptionalTargetPath};
use vrl::value::{KeyString, Value};

use crate::{
    internal_events::{PulsarPropertyExtractionError, TemplateRenderingError},
    sinks::pulsar::{config::PulsarSinkConfig, sink::PulsarEvent},
    template::ConfinedTemplate,
};

/// Transforms an event into a Pulsar event by rendering the required template fields.
/// Returns None if there is an error whilst rendering.
pub(super) fn make_pulsar_event(
    topic: &ConfinedTemplate,
    config: &PulsarSinkConfig,
    event: Event,
) -> Option<PulsarEvent> {
    let topic = topic
        .render_string(&event)
        .map_err(|error| {
            emit!(TemplateRenderingError {
                field: Some("topic"),
                drop_event: true,
                error,
            });
        })
        .ok()?;
    let key = get_key(&event, config.partition_key_field.as_ref());
    let timestamp_millis = get_timestamp_millis(&event);
    let properties = get_properties(&event, config.properties_key.as_ref());
    Some(PulsarEvent {
        event,
        topic,
        key,
        properties,
        timestamp_millis,
    })
}

fn get_key(event: &Event, partition_key_field: Option<&OptionalTargetPath>) -> Option<Bytes> {
    partition_key_field.and_then(|partition_key_field| match event {
        Event::Log(log) => partition_key_field
            .path
            .as_ref()
            .and_then(|path| log.get(path).map(vector_lib::event::Value::coerce_to_bytes)),
        Event::Metric(metric) => partition_key_field
            .path
            .as_ref()
            .and_then(|path| metric.tags().and_then(|tags| tags.get(&path.to_string())))
            .map(|value| value.to_owned().into()),
        Event::Trace(_) => None,
    })
}

fn get_timestamp_millis(event: &Event) -> Option<i64> {
    match &event {
        Event::Log(log) => log.get_timestamp().and_then(|v| v.as_timestamp()).copied(),
        Event::Metric(metric) => metric.timestamp(),
        Event::Trace(_) => None,
    }
    .map(|ts| ts.timestamp_millis())
}

pub(super) fn get_properties(
    event: &Event,
    properties_key: Option<&OptionalTargetPath>,
) -> Option<HashMap<KeyString, Bytes>> {
    properties_key.and_then(|properties_key| {
        properties_key.path.as_ref().and_then(|path| {
            event.maybe_as_log().and_then(|log| {
                log.get(path).and_then(|properties| {
                    if let Value::Object(headers_map) = properties {
                        let mut property_map = HashMap::new();
                        for (key, value) in headers_map {
                            if let Some(value_bytes) = value.as_bytes() {
                                property_map.insert(key.clone(), value_bytes.clone());
                            } else {
                                emit!(PulsarPropertyExtractionError {
                                    property_field: path
                                });
                            }
                        }
                        Some(property_map)
                    } else {
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
