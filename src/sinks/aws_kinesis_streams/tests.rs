#![cfg(test)]
//
// use super::*;
// use crate::{event::Event, test_util::random_string};
// use std::collections::BTreeMap;
//
// #[test]
// fn generate_config() {
//     crate::test_util::test_generate_config::<KinesisSinkConfig>();
// }
//
// #[test]
// fn kinesis_encode_event_text() {
//     let message = "hello world".to_string();
//     let event = encode_event(message.clone().into(), &None, &Encoding::Text.into()).unwrap();
//
//     assert_eq!(&event.item.data[..], message.as_bytes());
// }
//
// #[test]
// fn kinesis_encode_event_json() {
//     let message = "hello world".to_string();
//     let mut event = Event::from(message.clone());
//     event.as_mut_log().insert("key", "value");
//     let event = encode_event(event, &None, &Encoding::Json.into()).unwrap();
//
//     let map: BTreeMap<String, String> = serde_json::from_slice(&event.item.data[..]).unwrap();
//
//     assert_eq!(map[&log_schema().message_key().to_string()], message);
//     assert_eq!(map["key"], "value".to_string());
// }
//
// #[test]
// fn kinesis_encode_event_custom_partition_key() {
//     let mut event = Event::from("hello world");
//     event.as_mut_log().insert("key", "some_key");
//     let event = encode_event(event, &Some("key".into()), &Encoding::Text.into()).unwrap();
//
//     assert_eq!(&event.item.data[..], b"hello world");
//     assert_eq!(&event.item.partition_key, &"some_key".to_string());
// }
//
// #[test]
// fn kinesis_encode_event_custom_partition_key_limit() {
//     let mut event = Event::from("hello world");
//     event.as_mut_log().insert("key", random_string(300));
//     let event = encode_event(event, &Some("key".into()), &Encoding::Text.into()).unwrap();
//
//     assert_eq!(&event.item.data[..], b"hello world");
//     assert_eq!(event.item.partition_key.len(), 256);
// }
//
// #[test]
// fn kinesis_encode_event_apply_rules() {
//     let mut event = Event::from("hello world");
//     event.as_mut_log().insert("key", "some_key");
//
//     let mut encoding: EncodingConfig<_> = Encoding::Json.into();
//     encoding.except_fields = Some(vec!["key".into()]);
//
//     let event = encode_event(event, &Some("key".into()), &encoding).unwrap();
//     let map: BTreeMap<String, String> = serde_json::from_slice(&event.item.data[..]).unwrap();
//
//     assert_eq!(&event.item.partition_key, &"some_key".to_string());
//     assert!(!map.contains_key("key"));
// }
