use std::{future::ready, num::NonZeroUsize, pin::Pin};

use bytes::Bytes;
use futures::{Stream, StreamExt};
use lru::LruCache;
use vector_lib::config::{clone_input_definitions, LogNamespace};
use vector_lib::configurable::configurable_component;
use vector_lib::lookup::lookup_v2::ConfigTargetPath;
use vrl::path::OwnedTargetPath;

use crate::{
    config::{
        log_schema, DataType, GenerateConfig, Input, OutputId, TransformConfig, TransformContext,
        TransformOutput,
    },
    event::{Event, Value},
    internal_events::DedupeEventsDropped,
    schema,
    transforms::{TaskTransform, Transform},
};

/// Options to control what fields to match against.
///
/// When no field matching configuration is specified, events are matched using the `timestamp`,
/// `host`, and `message` fields from an event. The specific field names used are those set in
/// the global [`log schema`][global_log_schema] configuration.
///
/// [global_log_schema]: https://vector.dev/docs/reference/configuration/global-options/#log_schema
// TODO: This enum renders correctly in terms of providing equivalent Cue output when using the
// machine-generated stuff vs the previously-hand-written Cue... but what it _doesn't_ have in the
// machine-generated output is any sort of blurb that these "fields" (`match` and `ignore`) are
// actually mutually exclusive.
//
// We know that to be the case when we're generating the output from the configuration schema, so we
// need to emit something in that output to indicate as much, and further, actually use it on the
// Cue side to add some sort of boilerplate about them being mutually exclusive, etc.
#[configurable_component]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]
pub enum FieldMatchConfig {
    /// Matches events using only the specified fields.
    #[serde(rename = "match")]
    MatchFields(
        #[configurable(metadata(
            docs::examples = "field1",
            docs::examples = "parent.child_field"
        ))]
        Vec<ConfigTargetPath>,
    ),

    /// Matches events using all fields except for the ignored ones.
    #[serde(rename = "ignore")]
    IgnoreFields(
        #[configurable(metadata(
            docs::examples = "field1",
            docs::examples = "parent.child_field",
            docs::examples = "host",
            docs::examples = "hostname"
        ))]
        Vec<ConfigTargetPath>,
    ),
}

/// Caching configuration for deduplication.
#[configurable_component]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct CacheConfig {
    /// Number of events to cache and use for comparing incoming events to previously seen events.
    pub num_events: NonZeroUsize,
}

/// Configuration for the `dedupe` transform.
#[configurable_component(transform("dedupe", "Deduplicate logs passing through a topology."))]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct DedupeConfig {
    #[configurable(derived)]
    #[serde(default)]
    pub fields: Option<FieldMatchConfig>,

    #[configurable(derived)]
    #[serde(default = "default_cache_config")]
    pub cache: CacheConfig,
}

fn default_cache_config() -> CacheConfig {
    CacheConfig {
        num_events: NonZeroUsize::new(5000).expect("static non-zero number"),
    }
}

// TODO: Add support to the `configurable(metadata(..))` helper attribute for passing an expression
// that will provide the value for the metadata attribute's value, as well as letting all metadata
// attributes have whatever value they want, so long as it can be serialized by `serde_json`.
//
// Once we have that, we could curry these default values (and others) via a metadata attribute
// instead of via `serde(default = "...")` to allow for displaying default values in the
// configuration schema _without_ actually changing how a field is populated during deserialization.
//
// See the comment in `fill_default_fields_match` for more information on why this is required.
//
// TODO: These values are used even for events with the new "Vector" log namespace.
//   These aren't great defaults in that case, but hard-coding isn't much better since the
//   structure can vary significantly. This should probably either become a required field
//   in the future, or maybe the "semantic meaning" can be utilized here.
fn default_match_fields() -> Vec<ConfigTargetPath> {
    let mut fields = Vec::new();
    if let Some(message_key) = log_schema().message_key_target_path() {
        fields.push(ConfigTargetPath(message_key.clone()));
    }
    if let Some(host_key) = log_schema().host_key_target_path() {
        fields.push(ConfigTargetPath(host_key.clone()));
    }
    if let Some(timestamp_key) = log_schema().timestamp_key_target_path() {
        fields.push(ConfigTargetPath(timestamp_key.clone()));
    }
    fields
}

impl DedupeConfig {
    pub fn fill_default_fields_match(&self) -> FieldMatchConfig {
        // We provide a default value on `fields`, based on `default_match_fields`, in order to
        // drive the configuration schema and documentation. Since we're getting the values from the
        // configured log schema, though, the default field values shown in the configuration
        // schema/documentation may not be the same as an actual user's Vector configuration.
        match &self.fields {
            Some(FieldMatchConfig::MatchFields(x)) => FieldMatchConfig::MatchFields(x.clone()),
            Some(FieldMatchConfig::IgnoreFields(y)) => FieldMatchConfig::IgnoreFields(y.clone()),
            None => FieldMatchConfig::MatchFields(default_match_fields()),
        }
    }
}

pub struct Dedupe {
    fields: FieldMatchConfig,
    cache: LruCache<CacheEntry, bool>,
}

impl GenerateConfig for DedupeConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(Self {
            fields: None,
            cache: default_cache_config(),
        })
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "dedupe")]
impl TransformConfig for DedupeConfig {
    async fn build(&self, _context: &TransformContext) -> crate::Result<Transform> {
        Ok(Transform::event_task(Dedupe::new(self.clone())))
    }

    fn input(&self) -> Input {
        Input::log()
    }

    fn outputs(
        &self,
        _: vector_lib::enrichment::TableRegistry,
        input_definitions: &[(OutputId, schema::Definition)],
        _: LogNamespace,
    ) -> Vec<TransformOutput> {
        vec![TransformOutput::new(
            DataType::Log,
            clone_input_definitions(input_definitions),
        )]
    }
}

type TypeId = u8;

/// A CacheEntry comes in two forms, depending on the FieldMatchConfig in use.
///
/// When matching fields, a CacheEntry contains a vector of optional 2-tuples.
/// Each element in the vector represents one field in the corresponding
/// LogEvent. Elements in the vector will correspond 1:1 (and in order) to the
/// fields specified in "fields.match". The tuples each store the TypeId for
/// this field and the data as Bytes for the field. There is no need to store
/// the field name because the elements of the vector correspond 1:1 to
/// "fields.match", so there is never any ambiguity about what field is being
/// referred to. If a field from "fields.match" does not show up in an incoming
/// Event, the CacheEntry will have None in the correspond location in the
/// vector.
///
/// When ignoring fields, a CacheEntry contains a vector of 3-tuples. Each
/// element in the vector represents one field in the corresponding LogEvent.
/// The tuples will each contain the field name, TypeId, and data as Bytes for
/// the corresponding field (in that order). Since the set of fields that might
/// go into CacheEntries is not known at startup, we must store the field names
/// as part of CacheEntries. Since Event objects store their field in alphabetic
/// order (as they are backed by a BTreeMap), and we build CacheEntries by
/// iterating over the fields of the incoming Events, we know that the
/// CacheEntries for 2 equivalent events will always contain the fields in the
/// same order.
#[derive(PartialEq, Eq, Hash)]
enum CacheEntry {
    Match(Vec<Option<(TypeId, Bytes)>>),
    Ignore(Vec<(OwnedTargetPath, TypeId, Bytes)>),
}

/// Assigns a unique number to each of the types supported by Event::Value.
const fn type_id_for_value(val: &Value) -> TypeId {
    match val {
        Value::Bytes(_) => 0,
        Value::Timestamp(_) => 1,
        Value::Integer(_) => 2,
        Value::Float(_) => 3,
        Value::Boolean(_) => 4,
        Value::Object(_) => 5,
        Value::Array(_) => 6,
        Value::Null => 7,
        Value::Regex(_) => 8,
    }
}

impl Dedupe {
    pub fn new(config: DedupeConfig) -> Self {
        let num_entries = config.cache.num_events;
        let fields = config.fill_default_fields_match();
        Self {
            fields,
            cache: LruCache::new(num_entries),
        }
    }

    fn transform_one(&mut self, event: Event) -> Option<Event> {
        let cache_entry = build_cache_entry(&event, &self.fields);
        if self.cache.put(cache_entry, true).is_some() {
            emit!(DedupeEventsDropped { count: 1 });
            None
        } else {
            Some(event)
        }
    }
}

/// Takes in an Event and returns a CacheEntry to place into the LRU cache
/// containing all relevant information for the fields that need matching
/// against according to the specified FieldMatchConfig.
fn build_cache_entry(event: &Event, fields: &FieldMatchConfig) -> CacheEntry {
    match &fields {
        FieldMatchConfig::MatchFields(fields) => {
            let mut entry = Vec::new();
            for field_name in fields.iter() {
                if let Some(value) = event.as_log().get(field_name) {
                    entry.push(Some((type_id_for_value(value), value.coerce_to_bytes())));
                } else {
                    entry.push(None);
                }
            }
            CacheEntry::Match(entry)
        }
        FieldMatchConfig::IgnoreFields(fields) => {
            let mut entry = Vec::new();

            if let Some(event_fields) = event.as_log().all_event_fields() {
                if let Some(metadata_fields) = event.as_log().all_metadata_fields() {
                    for (field_name, value) in event_fields.chain(metadata_fields) {
                        if let Ok(path) = ConfigTargetPath::try_from(field_name) {
                            if !fields.contains(&path) {
                                entry.push((
                                    path.0,
                                    type_id_for_value(value),
                                    value.coerce_to_bytes(),
                                ));
                            }
                        }
                    }
                }
            }

            CacheEntry::Ignore(entry)
        }
    }
}

impl TaskTransform<Event> for Dedupe {
    fn transform(
        self: Box<Self>,
        task: Pin<Box<dyn Stream<Item = Event> + Send>>,
    ) -> Pin<Box<dyn Stream<Item = Event> + Send>>
    where
        Self: 'static,
    {
        let mut inner = self;
        Box::pin(task.filter_map(move |v| ready(inner.transform_one(v))))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use tokio::sync::mpsc;
    use tokio_stream::wrappers::ReceiverStream;
    use vector_lib::config::ComponentKey;
    use vector_lib::config::OutputId;
    use vector_lib::lookup::lookup_v2::ConfigTargetPath;

    use crate::config::schema::Definition;
    use crate::{
        event::{Event, LogEvent, ObjectMap, Value},
        test_util::components::assert_transform_compliance,
        transforms::{
            dedupe::{CacheConfig, DedupeConfig, FieldMatchConfig},
            test::create_topology,
        },
    };

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<DedupeConfig>();
    }

    fn make_match_transform_config(
        num_events: usize,
        fields: Vec<ConfigTargetPath>,
    ) -> DedupeConfig {
        DedupeConfig {
            cache: CacheConfig {
                num_events: std::num::NonZeroUsize::new(num_events).expect("non-zero num_events"),
            },
            fields: Some(FieldMatchConfig::MatchFields(fields)),
        }
    }

    fn make_ignore_transform_config(
        num_events: usize,
        given_fields: Vec<ConfigTargetPath>,
    ) -> DedupeConfig {
        // "message" and "timestamp" are added automatically to all Events
        let mut fields = vec!["message".into(), "timestamp".into()];
        fields.extend(given_fields);

        DedupeConfig {
            cache: CacheConfig {
                num_events: std::num::NonZeroUsize::new(num_events).expect("non-zero num_events"),
            },
            fields: Some(FieldMatchConfig::IgnoreFields(fields)),
        }
    }

    #[tokio::test]
    async fn dedupe_match_basic() {
        let transform_config = make_match_transform_config(5, vec!["matched".into()]);
        basic(transform_config, "matched", "unmatched").await;
    }

    #[tokio::test]
    async fn dedupe_ignore_basic() {
        let transform_config = make_ignore_transform_config(5, vec!["unmatched".into()]);
        basic(transform_config, "matched", "unmatched").await;
    }

    #[tokio::test]
    async fn dedupe_ignore_with_metadata_field() {
        let transform_config = make_ignore_transform_config(5, vec!["%ignored".into()]);
        basic(transform_config, "matched", "%ignored").await;
    }

    async fn basic(transform_config: DedupeConfig, first_path: &str, second_path: &str) {
        assert_transform_compliance(async {
            let (tx, rx) = mpsc::channel(1);
            let (topology, mut out) =
                create_topology(ReceiverStream::new(rx), transform_config).await;

            let mut event1 = Event::Log(LogEvent::from("message"));
            event1.as_mut_log().insert(first_path, "some value");
            event1.as_mut_log().insert(second_path, "another value");

            // Test that unmatched field isn't considered
            let mut event2 = Event::Log(LogEvent::from("message"));
            event2.as_mut_log().insert(first_path, "some value2");
            event2.as_mut_log().insert(second_path, "another value");

            // Test that matched field is considered
            let mut event3 = Event::Log(LogEvent::from("message"));
            event3.as_mut_log().insert(first_path, "some value");
            event3.as_mut_log().insert(second_path, "another value2");

            // First event should always be passed through as-is.
            tx.send(event1.clone()).await.unwrap();
            let new_event = out.recv().await.unwrap();

            event1.set_source_id(Arc::new(ComponentKey::from("in")));
            event1.set_upstream_id(Arc::new(OutputId::from("transform")));
            // the schema definition is copied from the source for dedupe
            event1
                .metadata_mut()
                .set_schema_definition(&Arc::new(Definition::default_legacy_namespace()));
            assert_eq!(new_event, event1);

            // Second event differs in matched field so should be output even though it
            // has the same value for unmatched field.
            tx.send(event2.clone()).await.unwrap();
            let new_event = out.recv().await.unwrap();

            event2.set_source_id(Arc::new(ComponentKey::from("in")));
            event2.set_upstream_id(Arc::new(OutputId::from("transform")));
            // the schema definition is copied from the source for dedupe
            event2
                .metadata_mut()
                .set_schema_definition(&Arc::new(Definition::default_legacy_namespace()));
            assert_eq!(new_event, event2);

            // Third event has the same value for "matched" as first event, so it should be dropped.
            tx.send(event3.clone()).await.unwrap();

            drop(tx);
            topology.stop().await;
            assert_eq!(out.recv().await, None);
        })
        .await;
    }

    #[tokio::test]
    async fn dedupe_match_field_name_matters() {
        let transform_config =
            make_match_transform_config(5, vec!["matched1".into(), "matched2".into()]);
        field_name_matters(transform_config).await;
    }

    #[tokio::test]
    async fn dedupe_ignore_field_name_matters() {
        let transform_config = make_ignore_transform_config(5, vec![]);
        field_name_matters(transform_config).await;
    }

    async fn field_name_matters(transform_config: DedupeConfig) {
        assert_transform_compliance(async {
            let (tx, rx) = mpsc::channel(1);
            let (topology, mut out) =
                create_topology(ReceiverStream::new(rx), transform_config).await;

            let mut event1 = Event::Log(LogEvent::from("message"));
            event1.as_mut_log().insert("matched1", "some value");

            let mut event2 = Event::Log(LogEvent::from("message"));
            event2.as_mut_log().insert("matched2", "some value");

            // First event should always be passed through as-is.
            tx.send(event1.clone()).await.unwrap();
            let new_event = out.recv().await.unwrap();

            event1.set_source_id(Arc::new(ComponentKey::from("in")));
            event1.set_upstream_id(Arc::new(OutputId::from("transform")));
            // the schema definition is copied from the source for dedupe
            event1
                .metadata_mut()
                .set_schema_definition(&Arc::new(Definition::default_legacy_namespace()));
            assert_eq!(new_event, event1);

            // Second event has a different matched field name with the same value,
            // so it should not be considered a dupe
            tx.send(event2.clone()).await.unwrap();
            let new_event = out.recv().await.unwrap();

            event2.set_source_id(Arc::new(ComponentKey::from("in")));
            event2.set_upstream_id(Arc::new(OutputId::from("transform")));
            // the schema definition is copied from the source for dedupe
            event2
                .metadata_mut()
                .set_schema_definition(&Arc::new(Definition::default_legacy_namespace()));
            assert_eq!(new_event, event2);

            drop(tx);
            topology.stop().await;
            assert_eq!(out.recv().await, None);
        })
        .await;
    }

    #[tokio::test]
    async fn dedupe_match_field_order_irrelevant() {
        let transform_config =
            make_match_transform_config(5, vec!["matched1".into(), "matched2".into()]);
        field_order_irrelevant(transform_config).await;
    }

    #[tokio::test]
    async fn dedupe_ignore_field_order_irrelevant() {
        let transform_config = make_ignore_transform_config(5, vec!["randomData".into()]);
        field_order_irrelevant(transform_config).await;
    }

    /// Test that two Events that are considered duplicates get handled that
    /// way, even if the order of the matched fields is different between the
    /// two.
    async fn field_order_irrelevant(transform_config: DedupeConfig) {
        assert_transform_compliance(async {
            let (tx, rx) = mpsc::channel(1);
            let (topology, mut out) =
                create_topology(ReceiverStream::new(rx), transform_config).await;

            let mut event1 = Event::Log(LogEvent::from("message"));
            event1.as_mut_log().insert("matched1", "value1");
            event1.as_mut_log().insert("matched2", "value2");

            // Add fields in opposite order
            let mut event2 = Event::Log(LogEvent::from("message"));
            event2.as_mut_log().insert("matched2", "value2");
            event2.as_mut_log().insert("matched1", "value1");

            // First event should always be passed through as-is.
            tx.send(event1.clone()).await.unwrap();
            let new_event = out.recv().await.unwrap();

            event1.set_source_id(Arc::new(ComponentKey::from("in")));
            event1.set_upstream_id(Arc::new(OutputId::from("transform")));
            // the schema definition is copied from the source for dedupe
            event1
                .metadata_mut()
                .set_schema_definition(&Arc::new(Definition::default_legacy_namespace()));
            assert_eq!(new_event, event1);

            // Second event is the same just with different field order, so it
            // shouldn't be output.
            tx.send(event2).await.unwrap();

            drop(tx);
            topology.stop().await;
            assert_eq!(out.recv().await, None);
        })
        .await;
    }

    #[tokio::test]
    async fn dedupe_match_age_out() {
        // Construct transform with a cache size of only 1 entry.
        let transform_config = make_match_transform_config(1, vec!["matched".into()]);
        age_out(transform_config).await;
    }

    #[tokio::test]
    async fn dedupe_ignore_age_out() {
        // Construct transform with a cache size of only 1 entry.
        let transform_config = make_ignore_transform_config(1, vec![]);
        age_out(transform_config).await;
    }

    /// Test the eviction behavior of the underlying LruCache
    async fn age_out(transform_config: DedupeConfig) {
        assert_transform_compliance(async {
            let (tx, rx) = mpsc::channel(1);
            let (topology, mut out) =
                create_topology(ReceiverStream::new(rx), transform_config).await;

            let mut event1 = Event::Log(LogEvent::from("message"));
            event1.as_mut_log().insert("matched", "some value");

            let mut event2 = Event::Log(LogEvent::from("message"));
            event2.as_mut_log().insert("matched", "some value2");

            // First event should always be passed through as-is.
            tx.send(event1.clone()).await.unwrap();
            let new_event = out.recv().await.unwrap();

            event1.set_source_id(Arc::new(ComponentKey::from("in")));
            event1.set_upstream_id(Arc::new(OutputId::from("transform")));

            // the schema definition is copied from the source for dedupe
            event1
                .metadata_mut()
                .set_schema_definition(&Arc::new(Definition::default_legacy_namespace()));
            assert_eq!(new_event, event1);

            // Second event gets output because it's not a dupe. This causes the first
            // Event to be evicted from the cache.
            tx.send(event2.clone()).await.unwrap();
            let new_event = out.recv().await.unwrap();

            event2.set_source_id(Arc::new(ComponentKey::from("in")));
            event2.set_upstream_id(Arc::new(OutputId::from("transform")));
            // the schema definition is copied from the source for dedupe
            event2
                .metadata_mut()
                .set_schema_definition(&Arc::new(Definition::default_legacy_namespace()));

            assert_eq!(new_event, event2);

            // Third event is a dupe but gets output anyway because the first
            // event has aged out of the cache.
            tx.send(event1.clone()).await.unwrap();
            let new_event = out.recv().await.unwrap();

            event1.set_source_id(Arc::new(ComponentKey::from("in")));
            assert_eq!(new_event, event1);

            drop(tx);
            topology.stop().await;
            assert_eq!(out.recv().await, None);
        })
        .await;
    }

    #[tokio::test]
    async fn dedupe_match_type_matching() {
        let transform_config = make_match_transform_config(5, vec!["matched".into()]);
        type_matching(transform_config).await;
    }

    #[tokio::test]
    async fn dedupe_ignore_type_matching() {
        let transform_config = make_ignore_transform_config(5, vec![]);
        type_matching(transform_config).await;
    }

    /// Test that two events with values for the matched fields that have
    /// different types but the same string representation aren't considered
    /// duplicates.
    async fn type_matching(transform_config: DedupeConfig) {
        assert_transform_compliance(async {
            let (tx, rx) = mpsc::channel(1);
            let (topology, mut out) =
                create_topology(ReceiverStream::new(rx), transform_config).await;

            let mut event1 = Event::Log(LogEvent::from("message"));
            event1.as_mut_log().insert("matched", "123");

            let mut event2 = Event::Log(LogEvent::from("message"));
            event2.as_mut_log().insert("matched", 123);

            // First event should always be passed through as-is.
            tx.send(event1.clone()).await.unwrap();
            let new_event = out.recv().await.unwrap();

            event1.set_source_id(Arc::new(ComponentKey::from("in")));
            event1.set_upstream_id(Arc::new(OutputId::from("transform")));
            // the schema definition is copied from the source for dedupe
            event1
                .metadata_mut()
                .set_schema_definition(&Arc::new(Definition::default_legacy_namespace()));
            assert_eq!(new_event, event1);

            // Second event should also get passed through even though the string
            // representations of "matched" are the same.
            tx.send(event2.clone()).await.unwrap();
            let new_event = out.recv().await.unwrap();

            event2.set_source_id(Arc::new(ComponentKey::from("in")));
            event2.set_upstream_id(Arc::new(OutputId::from("transform")));
            // the schema definition is copied from the source for dedupe
            event2
                .metadata_mut()
                .set_schema_definition(&Arc::new(Definition::default_legacy_namespace()));
            assert_eq!(new_event, event2);

            drop(tx);
            topology.stop().await;
            assert_eq!(out.recv().await, None);
        })
        .await;
    }

    #[tokio::test]
    async fn dedupe_match_type_matching_nested_objects() {
        let transform_config = make_match_transform_config(5, vec!["matched".into()]);
        type_matching_nested_objects(transform_config).await;
    }

    #[tokio::test]
    async fn dedupe_ignore_type_matching_nested_objects() {
        let transform_config = make_ignore_transform_config(5, vec![]);
        type_matching_nested_objects(transform_config).await;
    }

    /// Test that two events where the matched field is a sub object and that
    /// object contains values that have different types but the same string
    /// representation aren't considered duplicates.
    async fn type_matching_nested_objects(transform_config: DedupeConfig) {
        assert_transform_compliance(async {
            let (tx, rx) = mpsc::channel(1);
            let (topology, mut out) =
                create_topology(ReceiverStream::new(rx), transform_config).await;

            let mut map1 = ObjectMap::new();
            map1.insert("key".into(), "123".into());
            let mut event1 = Event::Log(LogEvent::from("message"));
            event1.as_mut_log().insert("matched", map1);

            let mut map2 = ObjectMap::new();
            map2.insert("key".into(), 123.into());
            let mut event2 = Event::Log(LogEvent::from("message"));
            event2.as_mut_log().insert("matched", map2);

            // First event should always be passed through as-is.
            tx.send(event1.clone()).await.unwrap();
            let new_event = out.recv().await.unwrap();

            event1.set_source_id(Arc::new(ComponentKey::from("in")));
            event1.set_upstream_id(Arc::new(OutputId::from("transform")));
            // the schema definition is copied from the source for dedupe
            event1
                .metadata_mut()
                .set_schema_definition(&Arc::new(Definition::default_legacy_namespace()));
            assert_eq!(new_event, event1);

            // Second event should also get passed through even though the string
            // representations of "matched" are the same.
            tx.send(event2.clone()).await.unwrap();
            let new_event = out.recv().await.unwrap();

            event2.set_source_id(Arc::new(ComponentKey::from("in")));
            event2.set_upstream_id(Arc::new(OutputId::from("transform")));
            // the schema definition is copied from the source for dedupe
            event2
                .metadata_mut()
                .set_schema_definition(&Arc::new(Definition::default_legacy_namespace()));
            assert_eq!(new_event, event2);

            drop(tx);
            topology.stop().await;
            assert_eq!(out.recv().await, None);
        })
        .await;
    }

    #[tokio::test]
    async fn dedupe_match_null_vs_missing() {
        let transform_config = make_match_transform_config(5, vec!["matched".into()]);
        ignore_vs_missing(transform_config).await;
    }

    #[tokio::test]
    async fn dedupe_ignore_null_vs_missing() {
        let transform_config = make_ignore_transform_config(5, vec![]);
        ignore_vs_missing(transform_config).await;
    }

    /// Test an explicit null vs a field being missing are treated as different.
    async fn ignore_vs_missing(transform_config: DedupeConfig) {
        assert_transform_compliance(async {
            let (tx, rx) = mpsc::channel(1);
            let (topology, mut out) =
                create_topology(ReceiverStream::new(rx), transform_config).await;

            let mut event1 = Event::Log(LogEvent::from("message"));
            event1.as_mut_log().insert("matched", Value::Null);

            let mut event2 = Event::Log(LogEvent::from("message"));

            // First event should always be passed through as-is.
            tx.send(event1.clone()).await.unwrap();
            let new_event = out.recv().await.unwrap();

            event1.set_source_id(Arc::new(ComponentKey::from("in")));
            event1.set_upstream_id(Arc::new(OutputId::from("transform")));
            // the schema definition is copied from the source for dedupe
            event1
                .metadata_mut()
                .set_schema_definition(&Arc::new(Definition::default_legacy_namespace()));
            assert_eq!(new_event, event1);

            // Second event should also get passed through as null is different than
            // missing
            tx.send(event2.clone()).await.unwrap();
            let new_event = out.recv().await.unwrap();

            event2.set_source_id(Arc::new(ComponentKey::from("in")));
            event2.set_upstream_id(Arc::new(OutputId::from("transform")));
            // the schema definition is copied from the source for dedupe
            event2
                .metadata_mut()
                .set_schema_definition(&Arc::new(Definition::default_legacy_namespace()));
            assert_eq!(new_event, event2);

            drop(tx);
            topology.stop().await;
            assert_eq!(out.recv().await, None);
        })
        .await;
    }
}
