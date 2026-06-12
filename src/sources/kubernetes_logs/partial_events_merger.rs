#![deny(missing_docs)]

use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

use bytes::BytesMut;
use futures::{Stream, StreamExt};
use vector_lib::codecs::OversizedAction;
use vector_lib::{
    config::LogNamespace,
    internal_event::{ComponentEventsDropped, INTENTIONAL},
    lookup::OwnedTargetPath,
    stream::expiration_map::{Emitter, map_with_expiration},
};
use vrl::owned_value_path;

use crate::{
    event,
    event::{Event, LogEvent, Value},
    internal_events::{KubernetesMergedLineTooBigError, KubernetesMergedLineTruncated},
    sources::kubernetes_logs::transform_utils::get_message_path,
};

/// The key we use for `file` field.
const FILE_KEY: &str = "file";

const EXPIRATION_TIME: Duration = Duration::from_secs(30);

const TRUNCATED_SUFFIX: &[u8] = b"..TRUNCATED";

struct PartialEventMergeState {
    buckets: HashMap<String, Bucket>,
    maybe_max_merged_line_bytes: Option<usize>,
    oversized_action: OversizedAction,
}

impl PartialEventMergeState {
    fn add_event(
        &mut self,
        event: LogEvent,
        file: &str,
        message_path: &OwnedTargetPath,
        expiration_time: Duration,
    ) {
        let mut bytes_mut = BytesMut::new();
        if let Some(bucket) = self.buckets.get_mut(file) {
            if bucket.exceeds_max_merged_line_limit {
                if !bucket.truncated {
                    emit!(ComponentEventsDropped::<INTENTIONAL> {
                        count: 1,
                        reason: "Partial event arrived after merged line exceeded max_merged_line_bytes limit.",
                    });
                }
                return;
            }

            if let (Some(Value::Bytes(prev_value)), Some(Value::Bytes(new_value))) =
                (bucket.event.get_mut(message_path), event.get(message_path))
            {
                bytes_mut.extend_from_slice(prev_value);
                bytes_mut.extend_from_slice(new_value);

                if let Some(max_merged_line_bytes) = self.maybe_max_merged_line_bytes
                    && bytes_mut.len() > max_merged_line_bytes
                {
                    bucket.exceeds_max_merged_line_limit = true;
                    match self.oversized_action {
                        OversizedAction::Drop => {
                            emit!(KubernetesMergedLineTooBigError {
                                event: &Value::Bytes(new_value.clone()),
                                configured_limit: max_merged_line_bytes,
                                encountered_size_so_far: bytes_mut.len()
                            });
                        }
                        OversizedAction::Truncate => {
                            let original_size = bytes_mut.len();
                            if max_merged_line_bytes > TRUNCATED_SUFFIX.len() {
                                bytes_mut.truncate(max_merged_line_bytes - TRUNCATED_SUFFIX.len());
                                bytes_mut.extend_from_slice(TRUNCATED_SUFFIX);
                            } else {
                                bytes_mut.truncate(max_merged_line_bytes);
                            }
                            bucket.truncated = true;
                            emit!(KubernetesMergedLineTruncated {
                                configured_limit: max_merged_line_bytes,
                                original_size,
                            });
                        }
                    }
                }

                if !bucket.exceeds_max_merged_line_limit || bucket.truncated {
                    *prev_value = bytes_mut.freeze();
                } else {
                    *prev_value = bytes::Bytes::new();
                }
            }
        } else {
            let mut exceeds_max_merged_line_limit = false;
            let mut truncated = false;

            if let Some(Value::Bytes(event_bytes)) = event.get(message_path) {
                bytes_mut.extend_from_slice(event_bytes);
                if let Some(max_merged_line_bytes) = self.maybe_max_merged_line_bytes
                    && bytes_mut.len() > max_merged_line_bytes
                {
                    exceeds_max_merged_line_limit = true;
                    match self.oversized_action {
                        OversizedAction::Drop => {
                            emit!(KubernetesMergedLineTooBigError {
                                event: &Value::Bytes(event_bytes.clone()),
                                configured_limit: max_merged_line_bytes,
                                encountered_size_so_far: bytes_mut.len()
                            });
                        }
                        OversizedAction::Truncate => {
                            let original_size = bytes_mut.len();
                            if max_merged_line_bytes > TRUNCATED_SUFFIX.len() {
                                bytes_mut.truncate(max_merged_line_bytes - TRUNCATED_SUFFIX.len());
                                bytes_mut.extend_from_slice(TRUNCATED_SUFFIX);
                            } else {
                                bytes_mut.truncate(max_merged_line_bytes);
                            }
                            truncated = true;
                            emit!(KubernetesMergedLineTruncated {
                                configured_limit: max_merged_line_bytes,
                                original_size,
                            });
                        }
                    }
                }
            }

            let mut event = event;
            if truncated {
                event.insert(message_path, Value::Bytes(bytes_mut.freeze()));
            }

            self.buckets.insert(
                file.to_owned(),
                Bucket {
                    event,
                    expiration: Instant::now() + expiration_time,
                    exceeds_max_merged_line_limit,
                    truncated,
                },
            );
        }
    }

    const fn should_emit(bucket: &Bucket) -> bool {
        !bucket.exceeds_max_merged_line_limit || bucket.truncated
    }

    fn remove_event(&mut self, file: &str) -> Option<LogEvent> {
        self.buckets
            .remove(file)
            .filter(Self::should_emit)
            .map(|bucket| bucket.event)
    }

    fn emit_expired_events(&mut self, emitter: &mut Emitter<LogEvent>) {
        let now = Instant::now();
        self.buckets.retain(|_key, bucket| {
            let expired = now >= bucket.expiration;
            if expired && Self::should_emit(bucket) {
                emitter.emit(bucket.event.clone());
            }
            !expired
        });
    }

    fn flush_events(&mut self, emitter: &mut Emitter<LogEvent>) {
        for (_, bucket) in self.buckets.drain() {
            if Self::should_emit(&bucket) {
                emitter.emit(bucket.event);
            }
        }
    }
}

struct Bucket {
    event: LogEvent,
    expiration: Instant,
    exceeds_max_merged_line_limit: bool,
    truncated: bool,
}

/// Merges partial events from a stream, with support for size limits and oversized behavior.
pub fn merge_partial_events(
    stream: impl Stream<Item = Event> + 'static,
    log_namespace: LogNamespace,
    maybe_max_merged_line_bytes: Option<usize>,
    oversized_action: OversizedAction,
) -> impl Stream<Item = Event> {
    merge_partial_events_with_custom_expiration(
        stream,
        log_namespace,
        EXPIRATION_TIME,
        maybe_max_merged_line_bytes,
        oversized_action,
    )
}

fn merge_partial_events_with_custom_expiration(
    stream: impl Stream<Item = Event> + 'static,
    log_namespace: LogNamespace,
    expiration_time: Duration,
    maybe_max_merged_line_bytes: Option<usize>,
    oversized_action: OversizedAction,
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
        maybe_max_merged_line_bytes,
        oversized_action,
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
            if !is_partial && let Some(log_event) = state.remove_event(&file) {
                emitter.emit(log_event);
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
    use vector_lib::event::LogEvent;
    use vrl::value;

    use super::*;

    #[tokio::test]
    async fn merge_single_event_legacy() {
        let mut e_1 = LogEvent::from("test message 1");
        e_1.insert("foo", 1);

        let input_stream = futures::stream::iter([e_1.into()]);
        let output_stream = merge_partial_events(
            input_stream,
            LogNamespace::Legacy,
            None,
            OversizedAction::Drop,
        );

        let output: Vec<Event> = output_stream.collect().await;
        assert_eq!(output.len(), 1);
        assert_eq!(
            output[0].as_log().get(".message"),
            Some(&value!("test message 1"))
        );
    }

    #[tokio::test]
    async fn merge_single_event_legacy_exceeds_max_merged_line_limit() {
        let mut e_1 = LogEvent::from("test message 1");
        e_1.insert("foo", 1);

        let input_stream = futures::stream::iter([e_1.into()]);
        let output_stream = merge_partial_events(
            input_stream,
            LogNamespace::Legacy,
            Some(1),
            OversizedAction::Drop,
        );

        let output: Vec<Event> = output_stream.collect().await;
        assert_eq!(output.len(), 0);
    }

    #[tokio::test]
    async fn merge_multiple_events_legacy() {
        let mut e_1 = LogEvent::from("test message 1");
        e_1.insert("foo", 1);
        e_1.insert("_partial", true);

        let mut e_2 = LogEvent::from("test message 2");
        e_2.insert("foo2", 1);

        let input_stream = futures::stream::iter([e_1.into(), e_2.into()]);
        let output_stream = merge_partial_events(
            input_stream,
            LogNamespace::Legacy,
            None,
            OversizedAction::Drop,
        );

        let output: Vec<Event> = output_stream.collect().await;
        assert_eq!(output.len(), 1);
        assert_eq!(
            output[0].as_log().get(".message"),
            Some(&value!("test message 1test message 2"))
        );
    }

    #[tokio::test]
    async fn merge_multiple_events_legacy_exceeds_max_merged_line_limit() {
        let mut e_1 = LogEvent::from("test message 1");
        e_1.insert("foo", 1);
        e_1.insert("_partial", true);

        let mut e_2 = LogEvent::from("test message 2");
        e_2.insert("foo2", 1);

        let input_stream = futures::stream::iter([e_1.into(), e_2.into()]);
        // 24 > length of first message but less than the two combined
        let output_stream = merge_partial_events(
            input_stream,
            LogNamespace::Legacy,
            Some(24),
            OversizedAction::Drop,
        );

        let output: Vec<Event> = output_stream.collect().await;
        assert_eq!(output.len(), 0);
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
        let output_stream = merge_partial_events(
            input_stream,
            LogNamespace::Legacy,
            None,
            OversizedAction::Drop,
        );

        let output: Vec<Event> = output_stream.collect().await;
        assert_eq!(output.len(), 1);
        assert_eq!(
            output[0].as_log().get(".message"),
            Some(&value!("test message 1test message 2"))
        );
    }

    #[tokio::test]
    async fn multiple_events_flush_legacy_exceeds_max_merged_line_limit() {
        let mut e_1 = LogEvent::from("test message 1");
        e_1.insert("foo", 1);
        e_1.insert("_partial", true);

        let mut e_2 = LogEvent::from("test message 2");
        e_2.insert("foo2", 1);
        e_1.insert("_partial", true);

        let input_stream = futures::stream::iter([e_1.into(), e_2.into()]);
        // 24 > length of first message but less than the two combined
        let output_stream = merge_partial_events(
            input_stream,
            LogNamespace::Legacy,
            Some(24),
            OversizedAction::Drop,
        );

        let output: Vec<Event> = output_stream.collect().await;
        assert_eq!(output.len(), 0);
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
            None,
            OversizedAction::Drop,
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
        let output_stream = merge_partial_events(
            input_stream,
            LogNamespace::Vector,
            None,
            OversizedAction::Drop,
        );

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
        let output_stream = merge_partial_events(
            input_stream,
            LogNamespace::Vector,
            None,
            OversizedAction::Drop,
        );

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

    #[tokio::test]
    async fn truncate_single_event_exceeding_limit() {
        let mut e_1 = LogEvent::from("test message 1");
        e_1.insert("foo", 1);

        let input_stream = futures::stream::iter([e_1.into()]);
        let output_stream = merge_partial_events(
            input_stream,
            LogNamespace::Legacy,
            Some(4),
            OversizedAction::Truncate,
        );

        let output: Vec<Event> = output_stream.collect().await;
        assert_eq!(output.len(), 1);
        assert_eq!(output[0].as_log().get(".message"), Some(&value!("test")));
    }

    #[tokio::test]
    async fn truncate_merged_events_exceeding_limit() {
        let mut e_1 = LogEvent::from("test message 1");
        e_1.insert("foo", 1);
        e_1.insert("_partial", true);

        let mut e_2 = LogEvent::from("test message 2");
        e_2.insert("foo2", 1);

        let input_stream = futures::stream::iter([e_1.into(), e_2.into()]);
        // 20 > "test message 1" (14 bytes) but < combined (28 bytes)
        let output_stream = merge_partial_events(
            input_stream,
            LogNamespace::Legacy,
            Some(20),
            OversizedAction::Truncate,
        );

        let output: Vec<Event> = output_stream.collect().await;
        assert_eq!(output.len(), 1);
        assert_eq!(
            output[0].as_log().get(".message"),
            Some(&value!("test mess..TRUNCATED"))
        );
    }

    #[tokio::test]
    async fn truncate_does_not_affect_events_within_limit() {
        let mut e_1 = LogEvent::from("short");
        e_1.insert("foo", 1);

        let input_stream = futures::stream::iter([e_1.into()]);
        let output_stream = merge_partial_events(
            input_stream,
            LogNamespace::Legacy,
            Some(100),
            OversizedAction::Truncate,
        );

        let output: Vec<Event> = output_stream.collect().await;
        assert_eq!(output.len(), 1);
        assert_eq!(output[0].as_log().get(".message"), Some(&value!("short")));
    }

    #[tokio::test]
    async fn truncate_discards_further_partials() {
        let mut e_1 = LogEvent::from("aaaa");
        e_1.insert("_partial", true);

        let mut e_2 = LogEvent::from("bbbb");
        e_2.insert("_partial", true);

        // Third event completes the merge — but e_2 should have been discarded
        let mut e_3 = LogEvent::from("cccc");
        e_3.insert("foo", 1);

        let input_stream = futures::stream::iter([e_1.into(), e_2.into(), e_3.into()]);
        // Limit at 6: "aaaa" (4) + "bbbb" (4) = 8 > 6, triggers truncation at merge
        let output_stream = merge_partial_events(
            input_stream,
            LogNamespace::Legacy,
            Some(6),
            OversizedAction::Truncate,
        );

        let output: Vec<Event> = output_stream.collect().await;
        // The truncated event is emitted when e_3 (non-partial) completes the sequence
        assert_eq!(output.len(), 1);
        assert_eq!(output[0].as_log().get(".message"), Some(&value!("aaaabb")));
    }

    #[tokio::test]
    async fn truncate_flush_emits_truncated_events() {
        let mut e_1 = LogEvent::from("test message 1");
        e_1.insert("foo", 1);
        e_1.insert("_partial", true);

        let mut e_2 = LogEvent::from("test message 2");
        e_2.insert("foo2", 1);
        e_2.insert("_partial", true);

        let input_stream = futures::stream::iter([e_1.into(), e_2.into()]);
        // Combined exceeds limit — truncated but emitted on flush
        let output_stream = merge_partial_events(
            input_stream,
            LogNamespace::Legacy,
            Some(20),
            OversizedAction::Truncate,
        );

        let output: Vec<Event> = output_stream.collect().await;
        assert_eq!(output.len(), 1);
        assert_eq!(
            output[0].as_log().get(".message"),
            Some(&value!("test mess..TRUNCATED"))
        );
    }
}
