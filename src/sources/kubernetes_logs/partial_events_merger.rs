#![deny(missing_docs)]

use enrichment::TableRegistry;
use indexmap::IndexMap;

use super::{transform_utils::optional::Optional, FILE_KEY};
use crate::{
    conditions::{AnyCondition, Condition, ConditionConfig, Conditional, ConditionalConfig},
    event,
    transforms::reduce::{MergeStrategy, Reduce, ReduceConfig},
};

#[derive(Clone, Debug, Default)]
struct HasNoPartialField;

impl Conditional for HasNoPartialField {
    fn check(&self, event: event::Event) -> (bool, event::Event) {
        let has_no_partial_field = match &event {
            event::Event::Log(log) => !log.contains(event::PARTIAL),
            _ => true,
        };

        (has_no_partial_field, event)
    }
}

impl ConditionalConfig for HasNoPartialField {
    fn build(&self, _: &enrichment::TableRegistry) -> crate::Result<Condition> {
        Ok(Condition::arbitrary(self.clone()))
    }
}

/// Partial event merger.
pub type PartialEventsMerger = Optional<Reduce>;

pub fn build(enabled: bool) -> PartialEventsMerger {
    let reducer = if enabled {
        // Merge the message field of each event by concatenating it, with a space delimiter.
        let mut merge_strategies = IndexMap::new();
        merge_strategies.insert(
            crate::config::log_schema().message_key().to_string(),
            MergeStrategy::Concat,
        );

        // Group events by their file and the partial indicator field.
        let group_by = vec![(&*FILE_KEY).to_string(), event::PARTIAL.to_string()];

        // As soon as we see an event that has no "partial" field, that's when we've hit the end of the split-up message
        // we've been incrementally aggregating.. or the message was never split up to begin with because it was already
        // small enough.
        let ends_when = Some(AnyCondition::Map(ConditionConfig::arbitrary(
            HasNoPartialField::default(),
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
