// #![cfg(test)]
//
// use super::*;
// use crate::aws::rusoto::RegionOrEndpoint;
// use crate::event::{Event, Value};
// use std::collections::HashMap;
// use std::convert::{TryFrom, TryInto};
//
// #[test]
// fn generate_config() {
//     crate::test_util::test_generate_config::<CloudwatchLogsSinkConfig>();
// }
//
// #[test]
// fn partition_static() {
//     let event = Event::from("hello world");
//     let stream = Template::try_from("stream").unwrap();
//     let group = "group".try_into().unwrap();
//     let encoding = Encoding::Text.into();
//
//     let encoded = partition_encode(event, &encoding, &group, &stream).unwrap();
//     let (_event, key) = encoded.item.into_parts();
//
//     let expected = CloudwatchKey {
//         stream: "stream".into(),
//         group: "group".into(),
//     };
//
//     assert_eq!(key, expected)
// }
//
// #[test]
// fn partition_event() {
//     let mut event = Event::from("hello world");
//
//     event.as_mut_log().insert("log_stream", "stream");
//
//     let stream = Template::try_from("{{log_stream}}").unwrap();
//     let group = "group".try_into().unwrap();
//     let encoding = Encoding::Text.into();
//
//     let encoded = partition_encode(event, &encoding, &group, &stream).unwrap();
//     let (_event, key) = encoded.item.into_parts();
//
//     let expected = CloudwatchKey {
//         stream: "stream".into(),
//         group: "group".into(),
//     };
//
//     assert_eq!(key, expected)
// }
//
// #[test]
// fn partition_event_with_prefix() {
//     let mut event = Event::from("hello world");
//
//     event.as_mut_log().insert("log_stream", "stream");
//
//     let stream = Template::try_from("abcd-{{log_stream}}").unwrap();
//     let group = "group".try_into().unwrap();
//     let encoding = Encoding::Text.into();
//
//     let encoded = partition_encode(event, &encoding, &group, &stream).unwrap();
//     let (_event, key) = encoded.item.into_parts();
//
//     let expected = CloudwatchKey {
//         stream: "abcd-stream".into(),
//         group: "group".into(),
//     };
//
//     assert_eq!(key, expected)
// }
//
// #[test]
// fn partition_event_with_postfix() {
//     let mut event = Event::from("hello world");
//
//     event.as_mut_log().insert("log_stream", "stream");
//
//     let stream = Template::try_from("{{log_stream}}-abcd").unwrap();
//     let group = "group".try_into().unwrap();
//     let encoding = Encoding::Text.into();
//
//     let encoded = partition_encode(event, &encoding, &group, &stream).unwrap();
//     let (_event, key) = encoded.item.into_parts();
//
//     let expected = CloudwatchKey {
//         stream: "stream-abcd".into(),
//         group: "group".into(),
//     };
//
//     assert_eq!(key, expected)
// }
//
// #[test]
// fn partition_no_key_event() {
//     let event = Event::from("hello world");
//
//     let stream = Template::try_from("{{log_stream}}").unwrap();
//     let group = "group".try_into().unwrap();
//     let encoding = Encoding::Text.into();
//
//     let stream_val = partition_encode(event, &encoding, &group, &stream);
//
//     assert!(stream_val.is_none());
// }
//
// fn svc(config: CloudwatchLogsSinkConfig) -> CloudwatchLogsSvc {
//     let config = CloudwatchLogsSinkConfig {
//         region: RegionOrEndpoint::with_endpoint("http://localhost:6000"),
//         ..config
//     };
//     let key = CloudwatchKey {
//         stream: "stream".into(),
//         group: "group".into(),
//     };
//     let client = config.create_client(&ProxyConfig::from_env()).unwrap();
//     CloudwatchLogsSvc::new(&config, &key, client)
// }
//
// #[test]
// fn cloudwatch_encoded_event_retains_timestamp() {
//     let mut event = Event::from("hello world").into_log();
//     event.insert("key", "value");
//     let encoded = encode_log(event.clone(), &Encoding::Json.into()).unwrap();
//
//     let ts = if let Value::Timestamp(ts) = event[log_schema().timestamp_key()] {
//         ts.timestamp_millis()
//     } else {
//         panic!()
//     };
//
//     assert_eq!(encoded.timestamp, ts);
// }
//
// #[test]
// fn cloudwatch_encode_log_as_json() {
//     let mut event = Event::from("hello world").into_log();
//     event.insert("key", "value");
//     let encoded = encode_log(event, &Encoding::Json.into()).unwrap();
//     let map: HashMap<String, String> = serde_json::from_str(&encoded.message[..]).unwrap();
//     assert!(map.get(log_schema().timestamp_key()).is_none());
// }
//
// #[test]
// fn cloudwatch_encode_log_as_text() {
//     let mut event = Event::from("hello world").into_log();
//     event.insert("key", "value");
//     let encoded = encode_log(event, &Encoding::Text.into()).unwrap();
//     assert_eq!(encoded.message, "hello world");
// }
//
// #[test]
// fn cloudwatch_24h_split() {
//     let now = Utc::now();
//     let events = (0..100)
//         .map(|i| now - Duration::hours(i))
//         .map(|timestamp| {
//             let mut event = Event::new_empty_log();
//             event
//                 .as_mut_log()
//                 .insert(log_schema().timestamp_key(), timestamp);
//             encode_log(event.into_log(), &Encoding::Text.into()).unwrap()
//         })
//         .collect();
//
//     let batches = svc(default_config(Encoding::Text)).process_events(events);
//
//     let day = Duration::days(1).num_milliseconds();
//     for batch in batches.iter() {
//         assert!((batch.last().unwrap().timestamp - batch.first().unwrap().timestamp) <= day);
//     }
//
//     assert_eq!(batches.len(), 5);
// }
