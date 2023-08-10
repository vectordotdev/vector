// use std::{collections::HashMap, convert::TryFrom};

// use codecs::{JsonSerializerConfig, TextSerializerConfig};
// use vector_core::event::LogEvent;

use super::config::RedisSinkConfig;
// use crate::config::log_schema;

#[test]
fn generate_config() {
    crate::test_util::test_generate_config::<RedisSinkConfig>();
}

// #[test]
// fn redis_event_json() {
//     let msg = "hello_world".to_owned();
//     let mut evt = LogEvent::from(msg.clone());
//     evt.insert("key", "value");
//     let result = encode_event(
//         evt.into(),
//         &Template::try_from("key").unwrap(),
//         &Default::default(),
//         &mut Encoder::<()>::new(JsonSerializerConfig::default().build().into()),
//     )
//     .unwrap()
//     .item
//     .value;
//     let map: HashMap<String, String> = serde_json::from_slice(&result[..]).unwrap();
//     assert_eq!(msg, map[&log_schema().message_key().unwrap().to_string()]);
// }

// #[test]
// fn redis_event_text() {
//     let msg = "hello_world".to_owned();
//     let evt = LogEvent::from(msg.clone());
//     let event = encode_event(
//         evt.into(),
//         &Template::try_from("key").unwrap(),
//         &Default::default(),
//         &mut Encoder::<()>::new(TextSerializerConfig::default().build().into()),
//     )
//     .unwrap()
//     .item
//     .value;
//     assert_eq!(event, Vec::from(msg));
// }

// #[test]
// fn redis_encode_event() {
//     let msg = "hello_world";
//     let mut evt = LogEvent::from(msg);
//     evt.insert("key", "value");

//     let result = encode_event(
//         evt.into(),
//         &Template::try_from("key").unwrap(),
//         &Transformer::new(None, Some(vec!["key".into()]), None).unwrap(),
//         &mut Encoder::<()>::new(JsonSerializerConfig::default().build().into()),
//     )
//     .unwrap()
//     .item
//     .value;

//     let map: HashMap<String, String> = serde_json::from_slice(&result[..]).unwrap();
//     assert!(!map.contains_key("key"));
// }
