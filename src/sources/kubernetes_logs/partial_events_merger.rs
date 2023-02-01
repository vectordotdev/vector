#![deny(missing_docs)]

use enrichment::TableRegistry;
use indexmap::IndexMap;
use vector_core::config::LogNamespace;

use super::{transform_utils::optional::Optional, FILE_KEY};
use crate::{
    conditions::AnyCondition,
    config::log_schema,
    event,
    transforms::reduce::{MergeStrategy, Reduce, ReduceConfig},
};

/// Partial event merger.
pub type PartialEventsMerger = Optional<Reduce>;

pub fn build(enabled: bool, log_namespace: LogNamespace) -> PartialEventsMerger {
    let reducer = if enabled {
        let key = match log_namespace {
            LogNamespace::Vector => ".".to_string(),
            LogNamespace::Legacy => log_schema().message_key().to_string(),
        };

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
        let reduce = Reduce::new(&reduce_config, &TableRegistry::default())
            .expect("should not fail to build `kubernetes_logs`-specific partial event reducer");

        Some(reduce)
    } else {
        None
    };

    Optional(reducer)
}
