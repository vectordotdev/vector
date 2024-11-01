use bytes::Bytes;
use vector_lib::configurable::component::GenerateConfig;
use vector_lib::lookup::lookup_v2::OptionalTargetPath;
use vrl::value::{ObjectMap, Value};

use crate::event::{Event, LogEvent};
use crate::sinks::pulsar::config::PulsarSinkConfig;

#[test]
fn generate_config() {
    PulsarSinkConfig::generate_config();
}

#[test]
fn pulsar_get_headers() {
    let properties_key = OptionalTargetPath::try_from("properties".to_string())
        .expect("unable to parse OptionalTargetPath");
    let mut property_values = ObjectMap::new();
    property_values.insert("a-key".into(), Value::Bytes(Bytes::from("a-value")));
    property_values.insert("b-key".into(), Value::Bytes(Bytes::from("b-value")));

    let mut event = Event::Log(LogEvent::from("hello"));
    event
        .as_mut_log()
        .insert(properties_key.path.as_ref().unwrap(), property_values);

    let properties = super::util::get_properties(&event, &Some(properties_key)).unwrap();
    assert_eq!(properties.get("a-key").unwrap(), "a-value".as_bytes());
    assert_eq!(properties.get("b-key").unwrap(), "b-value".as_bytes());
}
