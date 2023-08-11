#![deny(missing_docs)]
// TODO: temporary
#![allow(unused)]

use enrichment::TableRegistry;
use futures::Stream;
use indexmap::IndexMap;
use std::time::Duration;
use tokio_stream::wrappers::IntervalStream;
use vector_core::config::LogNamespace;

use crate::event::Event;
use crate::sources::kubernetes_logs::transform_utils::get_message_field;
use crate::{
    conditions::AnyCondition,
    event,
    transforms::reduce::{MergeStrategy, Reduce, ReduceConfig},
};

/// The key we use for `file` field.
const FILE_KEY: &str = "file";

/// Partial event merger.
pub type PartialEventsMerger = Reduce;

use async_stream::stream;

// enum MergeEvent {
//     Event(Event),
//     ExpirationCheck,
// }

pub fn merge_partial_events(stream: impl Stream<Item = Event>) -> impl Stream<Item = Event> {
    let (rx, tx) = futures::channel::mpsc::channel(10);

    // let expiration_check = IntervalStream::new());
    tokio::time::interval(Duration::from_secs(30));

    Box::pin(stream! {
        stream! {
              loop {
                let mut output = Vec::new();
                let done = tokio::select! {
                    _ = flush_stream.tick() => {
                      me.flush_into(&mut output);
                      false
                    }
                    maybe_event = input_rx.next() => {
                      match maybe_event {
                        None => {
                          me.flush_all_into(&mut output);
                          true
                        }
                        Some(event) => {
                          me.transform_one(&mut output, event);
                          false
                        }
                      }
                    }
                };
                yield futures::stream::iter(output.into_iter());
                if done { break }
              }
            }
            .flatten()
    })

    // tokio::spawn(async move { stream });

    // tx
}

pub fn build(log_namespace: LogNamespace) -> PartialEventsMerger {
    let key = get_message_field(log_namespace);

    // Merge the message field of each event by concatenating it, with a space delimiter.
    let mut merge_strategies = IndexMap::new();
    merge_strategies.insert(key, MergeStrategy::ConcatRaw);

    // Group events by their file.
    let group_by = vec![FILE_KEY.to_string()];

    // As soon as we see an event that has no "partial" field, that's when we've hit the end of the split-up message
    // we've been incrementally aggregating.. or the message was never split up to begin with because it was already
    // small enough.
    let ends_when = Some(AnyCondition::String(format!(
        "!exists(.{})",
        event::PARTIAL
    )));

    // This will default to expiring yet-to-be-completed reduced events after 30 seconds of inactivity, with an
    // interval of 1 second between checking if any reduced events have expired.
    let reduce_config = ReduceConfig {
        group_by,
        merge_strategies,
        ends_when,
        ..Default::default()
    };

    // TODO: This is _slightly_ gross because the semantics of `Reduce::new` could change and break things in a way
    // that isn't super visible in unit tests, if at all visible.
    Reduce::new(&reduce_config, &TableRegistry::default())
        .expect("should not fail to build `kubernetes_logs`-specific partial event reducer")
}

#[cfg(test)]
mod test {
    use super::*;
    use vector_core::event::LogEvent;

    #[tokio::test]
    async fn merge_single_event_legacy() {
        let mut merge = build(LogNamespace::Legacy);
        let mut output = vec![];

        let mut e_1 = LogEvent::from("test message 1");
        e_1.insert("foo", 1);
        merge.transform_one(&mut output, e_1.into());

        assert_eq!(output.len(), 1);
    }

    #[tokio::test]
    async fn merge_two_partial_events_legacy() {
        let mut merge = build(LogNamespace::Legacy);
        let mut output = vec![];

        let mut e_1 = LogEvent::from("test message 1");
        e_1.insert("foo", 1);
        e_1.insert("_partial", true);
        merge.transform_one(&mut output, e_1.into());

        assert_eq!(output.len(), 1);
    }
}
