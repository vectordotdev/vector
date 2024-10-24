use vector_lib::{
    config::{clone_input_definitions, LogNamespace},
    configurable::configurable_component,
};

use crate::{
    config::{
        DataType, GenerateConfig, Input, OutputId, TransformConfig, TransformContext,
        TransformOutput,
    },
    schema,
    transforms::Transform,
};

use super::{
    common::{default_cache_config, fill_default_fields_match, CacheConfig, FieldMatchConfig},
    transform::Dedupe,
};

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
        Ok(Transform::event_task(Dedupe::new(
            self.cache.num_events,
            fill_default_fields_match(self.fields.as_ref()),
        )))
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
            dedupe::config::{CacheConfig, DedupeConfig, FieldMatchConfig},
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
