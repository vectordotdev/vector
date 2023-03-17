use crate::internal_events::PulsarPropertyExtractionError;
use crate::sinks::pulsar::config::PulsarSinkConfig;
use crate::sinks::pulsar::sink::PulsarEvent;
use crate::template::Template;
use bytes::Bytes;
use std::collections::HashMap;
use value::Value;
use vector_core::event::Event;

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

fn get_key(event: &Event, partition_key_field: &Option<String>) -> Option<Bytes> {
    partition_key_field
        .as_ref()
        .and_then(|partition_key_field| match event {
            Event::Log(log) => log
                .get(partition_key_field.as_str())
                .map(|value| value.coerce_to_bytes()),
            Event::Metric(metric) => metric
                .tags()
                .and_then(|tags| tags.get(partition_key_field))
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
    properties_key: &Option<String>,
) -> Option<HashMap<String, Bytes>> {
    properties_key.as_ref().and_then(|properties_key| {
        if let Event::Log(log) = event {
            if let Some(properties) = log.get(properties_key.as_str()) {
                match properties {
                    Value::Object(headers_map) => {
                        let mut property_map = HashMap::new();
                        for (key, value) in headers_map {
                            if let Value::Bytes(value_bytes) = value {
                                property_map.insert(key.clone(), value_bytes.clone());
                            } else {
                                emit!(PulsarPropertyExtractionError {
                                    property_field: properties_key
                                });
                            }
                        }
                        return Some(property_map);
                    }
                    _ => {
                        emit!(PulsarPropertyExtractionError {
                            property_field: properties_key
                        });
                    }
                }
            }
        }
        None
    })
}
