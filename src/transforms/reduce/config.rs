use std::collections::HashMap;
use std::num::NonZeroUsize;
use std::time::Duration;

use indexmap::IndexMap;
use serde_with::serde_as;
use vrl::path::{parse_target_path, PathPrefix};
use vrl::prelude::{Collection, KeyString, Kind};

use vector_lib::configurable::configurable_component;

use crate::conditions::AnyCondition;
use crate::config::{
    schema, DataType, Input, LogNamespace, OutputId, TransformConfig, TransformContext,
    TransformOutput,
};
use crate::schema::Definition;
use crate::transforms::reduce::merge_strategy::MergeStrategy;
use crate::transforms::{reduce::transform::Reduce, Transform};

/// Configuration for the `reduce` transform.
#[serde_as]
#[configurable_component(transform(
"reduce",
"Collapse multiple log events into a single event based on a set of conditions and merge strategies.",
))]
#[derive(Clone, Debug, Derivative)]
#[derivative(Default)]
#[serde(deny_unknown_fields)]
pub struct ReduceConfig {
    /// The maximum period of time to wait after the last event is received, in milliseconds, before
    /// a combined event should be considered complete.
    #[serde(default = "default_expire_after_ms")]
    #[serde_as(as = "serde_with::DurationMilliSeconds<u64>")]
    #[derivative(Default(value = "default_expire_after_ms()"))]
    #[configurable(metadata(docs::human_name = "Expire After"))]
    pub expire_after_ms: Duration,

    /// If supplied, every time this interval elapses for a given grouping, the reduced value
    /// for that grouping is flushed. Checked every flush_period_ms.
    #[serde_as(as = "Option<serde_with::DurationMilliSeconds<u64>>")]
    #[derivative(Default(value = "Option::None"))]
    #[configurable(metadata(docs::human_name = "End-Every Period"))]
    pub end_every_period_ms: Option<Duration>,

    /// The interval to check for and flush any expired events, in milliseconds.
    #[serde(default = "default_flush_period_ms")]
    #[serde_as(as = "serde_with::DurationMilliSeconds<u64>")]
    #[derivative(Default(value = "default_flush_period_ms()"))]
    #[configurable(metadata(docs::human_name = "Flush Period"))]
    pub flush_period_ms: Duration,

    /// The maximum number of events to group together.
    pub max_events: Option<NonZeroUsize>,

    /// An ordered list of fields by which to group events.
    ///
    /// Each group with matching values for the specified keys is reduced independently, allowing
    /// you to keep independent event streams separate. When no fields are specified, all events
    /// are combined in a single group.
    ///
    /// For example, if `group_by = ["host", "region"]`, then all incoming events that have the same
    /// host and region are grouped together before being reduced.
    #[serde(default)]
    #[configurable(metadata(
        docs::examples = "request_id",
        docs::examples = "user_id",
        docs::examples = "transaction_id",
    ))]
    pub group_by: Vec<String>,

    /// A map of field names to custom merge strategies.
    ///
    /// For each field specified, the given strategy is used for combining events rather than
    /// the default behavior.
    ///
    /// The default behavior is as follows:
    ///
    /// - The first value of a string field is kept and subsequent values are discarded.
    /// - For timestamp fields the first is kept and a new field `[field-name]_end` is added with
    ///   the last received timestamp value.
    /// - Numeric values are summed.
    /// - For nested paths, the field value is retrieved and then reduced using the default strategies mentioned above (unless explicitly specified otherwise).
    #[serde(default)]
    #[configurable(metadata(
        docs::additional_props_description = "An individual merge strategy."
    ))]
    pub merge_strategies: IndexMap<KeyString, MergeStrategy>,

    /// A condition used to distinguish the final event of a transaction.
    ///
    /// If this condition resolves to `true` for an event, the current transaction is immediately
    /// flushed with this event.
    pub ends_when: Option<AnyCondition>,

    /// A condition used to distinguish the first event of a transaction.
    ///
    /// If this condition resolves to `true` for an event, the previous transaction is flushed
    /// (without this event) and a new transaction is started.
    pub starts_when: Option<AnyCondition>,
}

const fn default_expire_after_ms() -> Duration {
    Duration::from_millis(30000)
}

const fn default_flush_period_ms() -> Duration {
    Duration::from_millis(1000)
}

impl_generate_config_from_default!(ReduceConfig);

#[async_trait::async_trait]
#[typetag::serde(name = "reduce")]
impl TransformConfig for ReduceConfig {
    async fn build(&self, context: &TransformContext) -> crate::Result<Transform> {
        Reduce::new(self, &context.enrichment_tables, &context.vrl_caches)
            .map(Transform::event_task)
    }

    fn input(&self) -> Input {
        Input::log()
    }

    fn outputs(
        &self,
        _: vector_lib::enrichment::TableRegistry,
        _: vector_lib::vrl_cache::VrlCacheRegistry,
        input_definitions: &[(OutputId, schema::Definition)],
        _: LogNamespace,
    ) -> Vec<TransformOutput> {
        // Events may be combined, so there isn't a true single "source" for events.
        // All of the definitions must be merged.
        let merged_definition: Definition = input_definitions
            .iter()
            .map(|(_output, definition)| definition.clone())
            .reduce(Definition::merge)
            .unwrap_or_else(Definition::any);

        let mut schema_definition = merged_definition;

        for (key, merge_strategy) in self.merge_strategies.iter() {
            let key = if let Ok(key) = parse_target_path(key) {
                key
            } else {
                continue;
            };

            let input_kind = match key.prefix {
                PathPrefix::Event => schema_definition.event_kind().at_path(&key.path),
                PathPrefix::Metadata => schema_definition.metadata_kind().at_path(&key.path),
            };

            let new_kind = match merge_strategy {
                MergeStrategy::Discard | MergeStrategy::Retain => {
                    /* does not change the type */
                    input_kind.clone()
                }
                MergeStrategy::Sum | MergeStrategy::Max | MergeStrategy::Min => {
                    // only keeps integer / float values
                    match (input_kind.contains_integer(), input_kind.contains_float()) {
                        (true, true) => Kind::float().or_integer(),
                        (true, false) => Kind::integer(),
                        (false, true) => Kind::float(),
                        (false, false) => Kind::undefined(),
                    }
                }
                MergeStrategy::Array => {
                    let unknown_kind = input_kind.clone();
                    Kind::array(Collection::empty().with_unknown(unknown_kind))
                }
                MergeStrategy::Concat => {
                    let mut new_kind = Kind::never();

                    if input_kind.contains_bytes() {
                        new_kind.add_bytes();
                    }
                    if let Some(array) = input_kind.as_array() {
                        // array elements can be either any type that the field can be, or any
                        // element of the array
                        let array_elements = array.reduced_kind().union(input_kind.without_array());
                        new_kind.add_array(Collection::empty().with_unknown(array_elements));
                    }
                    new_kind
                }
                MergeStrategy::ConcatNewline | MergeStrategy::ConcatRaw => {
                    // can only produce bytes (or undefined)
                    if input_kind.contains_bytes() {
                        Kind::bytes()
                    } else {
                        Kind::undefined()
                    }
                }
                MergeStrategy::ShortestArray | MergeStrategy::LongestArray => {
                    if let Some(array) = input_kind.as_array() {
                        Kind::array(array.clone())
                    } else {
                        Kind::undefined()
                    }
                }
                MergeStrategy::FlatUnique => {
                    let mut array_elements = input_kind.without_array().without_object();
                    if let Some(array) = input_kind.as_array() {
                        array_elements = array_elements.union(array.reduced_kind());
                    }
                    if let Some(object) = input_kind.as_object() {
                        array_elements = array_elements.union(object.reduced_kind());
                    }
                    Kind::array(Collection::empty().with_unknown(array_elements))
                }
            };

            // all of the merge strategies are optional. They won't produce a value unless a value actually exists
            let new_kind = if input_kind.contains_undefined() {
                new_kind.or_undefined()
            } else {
                new_kind
            };

            schema_definition = schema_definition.with_field(&key, new_kind, None);
        }

        // the same schema definition is used for all inputs
        let mut output_definitions = HashMap::new();
        for (output, _input) in input_definitions {
            output_definitions.insert(output.clone(), schema_definition.clone());
        }

        vec![TransformOutput::new(DataType::Log, output_definitions)]
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<ReduceConfig>();
    }
}
