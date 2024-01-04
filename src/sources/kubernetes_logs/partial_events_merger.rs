#![deny(missing_docs)]

use bytes::BytesMut;
use futures::{Stream, StreamExt};
use std::collections::HashMap;
use std::time::{Duration, Instant};
use vector_lib::config::LogNamespace;
use vector_lib::lookup::OwnedTargetPath;
use vector_lib::stream::expiration_map::{map_with_expiration, Emitter};
use vrl::owned_value_path;

use crate::event;
use crate::event::{Event, LogEvent, Value};
use crate::sources::kubernetes_logs::transform_utils::get_message_path;

/// The key we use for `file` field.
const FILE_KEY: &str = "file";

const EXPIRATION_TIME: Duration = Duration::from_secs(30);

struct PartialEventMergeState {
    buckets: HashMap<String, Bucket>,
}

impl PartialEventMergeState {
    fn add_event(
        &mut self,
        event: LogEvent,
        file: &str,
        message_path: &OwnedTargetPath,
        expiration_time: Duration,
    ) {
        if let Some(bucket) = self.buckets.get_mut(file) {
            // merging with existing event

            if let (Some(Value::Bytes(prev_value)), Some(Value::Bytes(new_value))) =
                (bucket.event.get_mut(message_path), event.get(message_path))
            {
                let mut bytes_mut = BytesMut::new();
                bytes_mut.extend_from_slice(prev_value);
                bytes_mut.extend_from_slice(new_value);
                *prev_value = bytes_mut.freeze();
            }
        } else {
            // new event
            self.buckets.insert(
                file.to_owned(),
                Bucket {
                    event,
                    expiration: Instant::now() + expiration_time,
                },
            );
        }
    }

    fn remove_event(&mut self, file: &str) -> Option<LogEvent> {
        self.buckets.remove(file).map(|bucket| bucket.event)
    }

    fn emit_expired_events(&mut self, emitter: &mut Emitter<LogEvent>) {
        let now = Instant::now();
        self.buckets.retain(|_key, bucket| {
            let expired = now >= bucket.expiration;
            if expired {
                emitter.emit(bucket.event.clone());
            }
            !expired
        });
    }

    fn flush_events(&mut self, emitter: &mut Emitter<LogEvent>) {
        for (_, bucket) in self.buckets.drain() {
            emitter.emit(bucket.event);
        }
    }
}

struct Bucket {
    event: LogEvent,
    expiration: Instant,
}

pub fn merge_partial_events(
    stream: impl Stream<Item = Event> + 'static,
    log_namespace: LogNamespace,
) -> impl Stream<Item = Event> {
    merge_partial_events_with_custom_expiration(stream, log_namespace, EXPIRATION_TIME)
}

// internal function that allows customizing the expiration time (for testing)
fn merge_partial_events_with_custom_expiration(
    stream: impl Stream<Item = Event> + 'static,
    log_namespace: LogNamespace,
    expiration_time: Duration,
) -> impl Stream<Item = Event> {
    let partial_flag_path = match log_namespace {
        LogNamespace::Vector => {
            OwnedTargetPath::metadata(owned_value_path!(super::Config::NAME, event::PARTIAL))
        }
        LogNamespace::Legacy => OwnedTargetPath::event(owned_value_path!(event::PARTIAL)),
    };

    let file_path = match log_namespace {
        LogNamespace::Vector => {
            OwnedTargetPath::metadata(owned_value_path!(super::Config::NAME, FILE_KEY))
        }
        LogNamespace::Legacy => OwnedTargetPath::event(owned_value_path!(FILE_KEY)),
    };

    let state = PartialEventMergeState {
        buckets: HashMap::new(),
    };

    let message_path = get_message_path(log_namespace);

    map_with_expiration(
        state,
        stream.map(|e| e.into_log()),
        Duration::from_secs(1),
        move |state: &mut PartialEventMergeState,
              event: LogEvent,
              emitter: &mut Emitter<LogEvent>| {
            // called for each event
            let is_partial = event
                .get(&partial_flag_path)
                .and_then(|x| x.as_boolean())
                .unwrap_or(false);

            let file = event
                .get(&file_path)
                .and_then(|x| x.as_str())
                .map(|x| x.to_string())
                .unwrap_or_default();

            state.add_event(event, &file, &message_path, expiration_time);
            if !is_partial {
                if let Some(log_event) = state.remove_event(&file) {
                    emitter.emit(log_event);
                }
            }
        },
        |state: &mut PartialEventMergeState, emitter: &mut Emitter<LogEvent>| {
            // check for expired events
            state.emit_expired_events(emitter)
        },
        |state: &mut PartialEventMergeState, emitter: &mut Emitter<LogEvent>| {
            // the source is ending, flush all pending events
            state.flush_events(emitter);
        },
    )
    // LogEvent -> Event
    .map(|e| e.into())
}

#[cfg(test)]
mod test {
    use super::*;
    use vector_lib::event::LogEvent;
    use vrl::value;

    #[tokio::test]
    async fn merge_single_event_legacy() {
        let mut e_1 = LogEvent::from("test message 1");
        e_1.insert("foo", 1);

        let input_stream = futures::stream::iter([e_1.into()]);
        let output_stream = merge_partial_events(input_stream, LogNamespace::Legacy);

        let output: Vec<Event> = output_stream.collect().await;
        assert_eq!(output.len(), 1);
        assert_eq!(
            output[0].as_log().get(".message"),
            Some(&value!("test message 1"))
        );
    }

    #[tokio::test]
    async fn merge_multiple_events_legacy() {
        let mut e_1 = LogEvent::from("test message 1");
        e_1.insert("foo", 1);
        e_1.insert("_partial", true);

        let mut e_2 = LogEvent::from("test message 2");
        e_2.insert("foo2", 1);

        let input_stream = futures::stream::iter([e_1.into(), e_2.into()]);
        let output_stream = merge_partial_events(input_stream, LogNamespace::Legacy);

        let output: Vec<Event> = output_stream.collect().await;
        assert_eq!(output.len(), 1);
        assert_eq!(
            output[0].as_log().get(".message"),
            Some(&value!("test message 1test message 2"))
        );
    }

    #[tokio::test]
    async fn multiple_events_flush_legacy() {
        let mut e_1 = LogEvent::from("test message 1");
        e_1.insert("foo", 1);
        e_1.insert("_partial", true);

        let mut e_2 = LogEvent::from("test message 2");
        e_2.insert("foo2", 1);
        e_1.insert("_partial", true);

        let input_stream = futures::stream::iter([e_1.into(), e_2.into()]);
        let output_stream = merge_partial_events(input_stream, LogNamespace::Legacy);

        let output: Vec<Event> = output_stream.collect().await;
        assert_eq!(output.len(), 1);
        assert_eq!(
            output[0].as_log().get(".message"),
            Some(&value!("test message 1test message 2"))
        );
    }

    #[tokio::test]
    async fn multiple_events_expire_legacy() {
        let mut e_1 = LogEvent::from("test message");
        e_1.insert(FILE_KEY, "foo1");
        e_1.insert("_partial", true);

        let mut e_2 = LogEvent::from("test message");
        e_2.insert(FILE_KEY, "foo2");
        e_1.insert("_partial", true);

        // and input stream that never ends
        let input_stream =
            futures::stream::iter([e_1.into(), e_2.into()]).chain(futures::stream::pending());

        let output_stream = merge_partial_events_with_custom_expiration(
            input_stream,
            LogNamespace::Legacy,
            Duration::from_secs(1),
        );

        let output: Vec<Event> = output_stream.take(2).collect().await;
        assert_eq!(output.len(), 2);
        assert_eq!(
            output[0].as_log().get(".message"),
            Some(&value!("test message"))
        );
        assert_eq!(
            output[1].as_log().get(".message"),
            Some(&value!("test message"))
        );
    }

    #[tokio::test]
    async fn merge_single_event_vector_namespace() {
        let mut e_1 = LogEvent::from(value!("test message 1"));
        e_1.insert(
            vrl::metadata_path!(super::super::Config::NAME, FILE_KEY),
            "foo1",
        );

        let input_stream = futures::stream::iter([e_1.into()]);
        let output_stream = merge_partial_events(input_stream, LogNamespace::Vector);

        let output: Vec<Event> = output_stream.collect().await;
        assert_eq!(output.len(), 1);
        assert_eq!(output[0].as_log().get("."), Some(&value!("test message 1")));
        assert_eq!(
            output[0].as_log().get("%kubernetes_logs.file"),
            Some(&value!("foo1"))
        );
    }

    #[tokio::test]
    async fn merge_multiple_events_vector_namespace() {
        let mut e_1 = LogEvent::from(value!("test message 1"));
        e_1.insert(
            vrl::metadata_path!(super::super::Config::NAME, "_partial"),
            true,
        );
        e_1.insert(
            vrl::metadata_path!(super::super::Config::NAME, FILE_KEY),
            "foo1",
        );

        let mut e_2 = LogEvent::from(value!("test message 2"));
        e_2.insert(
            vrl::metadata_path!(super::super::Config::NAME, FILE_KEY),
            "foo1",
        );

        let input_stream = futures::stream::iter([e_1.into(), e_2.into()]);
        let output_stream = merge_partial_events(input_stream, LogNamespace::Vector);

        let output: Vec<Event> = output_stream.collect().await;
        assert_eq!(output.len(), 1);
        assert_eq!(
            output[0].as_log().get("."),
            Some(&value!("test message 1test message 2"))
        );
        assert_eq!(
            output[0].as_log().get("%kubernetes_logs.file"),
            Some(&value!("foo1"))
        );
    }
}
