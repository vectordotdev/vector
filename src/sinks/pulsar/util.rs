use crate::internal_events::PulsarPropertyExtractionError;
use crate::sinks::pulsar::config::PulsarSinkConfig;
use crate::sinks::pulsar::sink::PulsarEvent;
use crate::template::Template;
use bytes::Bytes;
use std::collections::HashMap;
use value::Value;
use vector_core::config::log_schema;
use vector_core::config::LogSchema;
use vector_core::event::Event;

/// Transforms an event into a Pulsar event by rendering the required template fields.
/// Returns None if there is an error whilst rendering.
pub(super) fn make_pulsar_event(
    topic: &Template,
    config: &PulsarSinkConfig,
    event: Event,
) -> Option<PulsarEvent> {
    let topic = topic.render_string(&event).ok()?;
    let key = get_key(&event, &config.key_field);
    let timestamp_millis = get_timestamp_millis(&event, log_schema());
    let properties = get_properties(&event, &config.properties_key);
    Some(PulsarEvent {
        event,
        topic,
        key,
        timestamp_millis,
        properties,
    })
}

fn get_key(event: &Event, key_field: &Option<String>) -> Option<Bytes> {
    key_field.as_ref().and_then(|key_field| match event {
        Event::Log(log) => log
            .get(key_field.as_str())
            .map(|value| value.coerce_to_bytes()),
        Event::Metric(metric) => metric
            .tags()
            .and_then(|tags| tags.get(key_field))
            .map(|value| value.to_owned().into()),
        _ => None,
    })
}

fn get_timestamp_millis(event: &Event, log_schema: &'static LogSchema) -> Option<i64> {
    match &event {
        Event::Log(log) => log
            .get(log_schema.timestamp_key())
            .and_then(|v| v.as_timestamp())
            .copied(),
        Event::Metric(metric) => metric.timestamp(),
        _ => None,
    }
    .map(|ts| ts.timestamp_millis())
}

fn get_properties(
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

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use bytes::Bytes;

    use super::*;
    use crate::event::LogEvent;

    #[test]
    fn pulsar_get_headers() {
        let properties_key = "properties";
        let mut property_values = BTreeMap::new();
        property_values.insert("a-key".to_string(), Value::Bytes(Bytes::from("a-value")));
        property_values.insert("b-key".to_string(), Value::Bytes(Bytes::from("b-value")));

        let mut event = Event::Log(LogEvent::from("hello"));
        event.as_mut_log().insert(properties_key, property_values);

        let properties = get_properties(&event, &Some(properties_key.to_string())).unwrap();
        assert_eq!(properties.get("a-key").unwrap(), "a-value".as_bytes());
        assert_eq!(properties.get("b-key").unwrap(), "b-value".as_bytes());
    }
}
