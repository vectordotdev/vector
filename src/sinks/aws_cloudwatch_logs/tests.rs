#![cfg(test)]

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
//             let mut buffer = vec![];
//             StandardEncodings::Text.encode_input(event.into_log(), &mut buffer);
//
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
