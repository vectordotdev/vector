use std::collections::HashMap;
use std::sync::Arc;

use config::PerMetricConfig;
use vector_lib::config::ComponentKey;
use vector_lib::config::OutputId;
use vector_lib::event::EventMetadata;
use vector_lib::metric_tags;

use super::*;
use crate::config::schema::Definition;
use crate::config::LogNamespace;
use crate::event::metric::TagValue;
use crate::event::{metric, Event, Metric, MetricTags};
use crate::test_util::components::assert_transform_compliance;
use crate::transforms::tag_cardinality_limit::config::{
    default_cache_size, BloomFilterConfig, Mode,
};
use crate::transforms::test::create_topology;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use vrl::compiler::prelude::Kind;

#[test]
fn generate_config() {
    crate::test_util::test_generate_config::<TagCardinalityLimitConfig>();
}

fn make_metric_with_name(tags: MetricTags, name: &str) -> Event {
    let event_metadata = EventMetadata::default().with_source_type("unit_test_stream");

    Event::Metric(
        Metric::new_with_metadata(
            name,
            metric::MetricKind::Incremental,
            metric::MetricValue::Counter { value: 1.0 },
            event_metadata,
        )
        .with_tags(Some(tags)),
    )
}

fn make_metric(tags: MetricTags) -> Event {
    make_metric_with_name(tags, "event")
}

fn make_transform_hashset(
    value_limit: usize,
    limit_exceeded_action: LimitExceededAction,
) -> TagCardinalityLimitConfig {
    TagCardinalityLimitConfig {
        global: TagCardinalityLimitInnerConfig {
            value_limit,
            limit_exceeded_action,
            mode: Mode::Exact,
        },
        per_metric_limits: HashMap::new(),
    }
}

fn make_transform_bloom(
    value_limit: usize,
    limit_exceeded_action: LimitExceededAction,
) -> TagCardinalityLimitConfig {
    TagCardinalityLimitConfig {
        global: TagCardinalityLimitInnerConfig {
            value_limit,
            limit_exceeded_action,
            mode: Mode::Probabilistic(BloomFilterConfig {
                cache_size_per_key: default_cache_size(),
            }),
        },
        per_metric_limits: HashMap::new(),
    }
}

const fn make_transform_hashset_with_per_metric_limits(
    value_limit: usize,
    limit_exceeded_action: LimitExceededAction,
    per_metric_limits: HashMap<String, PerMetricConfig>,
) -> TagCardinalityLimitConfig {
    TagCardinalityLimitConfig {
        global: TagCardinalityLimitInnerConfig {
            value_limit,
            limit_exceeded_action,
            mode: Mode::Exact,
        },
        per_metric_limits,
    }
}

const fn make_transform_bloom_with_per_metric_limits(
    value_limit: usize,
    limit_exceeded_action: LimitExceededAction,
    per_metric_limits: HashMap<String, PerMetricConfig>,
) -> TagCardinalityLimitConfig {
    TagCardinalityLimitConfig {
        global: TagCardinalityLimitInnerConfig {
            value_limit,
            limit_exceeded_action,
            mode: Mode::Probabilistic(BloomFilterConfig {
                cache_size_per_key: default_cache_size(),
            }),
        },
        per_metric_limits,
    }
}

#[tokio::test]
async fn tag_cardinality_limit_drop_event_hashset() {
    drop_event(make_transform_hashset(2, LimitExceededAction::DropEvent)).await;
}

#[tokio::test]
async fn tag_cardinality_limit_drop_event_bloom() {
    drop_event(make_transform_bloom(2, LimitExceededAction::DropEvent)).await;
}

async fn drop_event(config: TagCardinalityLimitConfig) {
    assert_transform_compliance(async move {
        let mut event1 = make_metric(metric_tags!("tag1" => "val1"));
        let mut event2 = make_metric(metric_tags!("tag1" => "val2"));
        let event3 = make_metric(metric_tags!("tag1" => "val3"));

        let (tx, rx) = mpsc::channel(1);
        let (topology, mut out) = create_topology(ReceiverStream::new(rx), config).await;

        tx.send(event1.clone()).await.unwrap();
        tx.send(event2.clone()).await.unwrap();
        tx.send(event3.clone()).await.unwrap();

        let new_event1 = out.recv().await;
        let new_event2 = out.recv().await;

        drop(tx);
        topology.stop().await;

        let new_event3 = out.recv().await;

        event1.set_source_id(Arc::new(ComponentKey::from("in")));
        event2.set_source_id(Arc::new(ComponentKey::from("in")));

        event1.set_upstream_id(Arc::new(OutputId::from("transform")));
        event2.set_upstream_id(Arc::new(OutputId::from("transform")));

        event1.metadata_mut().set_schema_definition(&Arc::new(
            Definition::new_with_default_metadata(Kind::any_object(), [LogNamespace::Legacy]),
        ));
        event2.metadata_mut().set_schema_definition(&Arc::new(
            Definition::new_with_default_metadata(Kind::any_object(), [LogNamespace::Legacy]),
        ));

        assert_eq!(new_event1, Some(event1));
        assert_eq!(new_event2, Some(event2));
        // Third value rejected since value_limit is 2.
        assert_eq!(None, new_event3);
    })
    .await;
}

#[tokio::test]
async fn tag_cardinality_limit_drop_tag_hashset() {
    drop_tag(make_transform_hashset(2, LimitExceededAction::DropTag)).await;
}

#[tokio::test]
async fn tag_cardinality_limit_drop_tag_bloom() {
    drop_tag(make_transform_bloom(2, LimitExceededAction::DropTag)).await;
}

async fn drop_tag(config: TagCardinalityLimitConfig) {
    assert_transform_compliance(async move {
        let tags1 = metric_tags!("tag1" => "val1", "tag2" => "val1");
        let mut event1 = make_metric(tags1);

        let tags2 = metric_tags!("tag1" => "val2", "tag2" => "val1");
        let mut event2 = make_metric(tags2);

        let tags3 = metric_tags!("tag1" => "val3", "tag2" => "val1");
        let mut event3 = make_metric(tags3);

        let (tx, rx) = mpsc::channel(1);
        let (topology, mut out) = create_topology(ReceiverStream::new(rx), config).await;

        tx.send(event1.clone()).await.unwrap();
        tx.send(event2.clone()).await.unwrap();
        tx.send(event3.clone()).await.unwrap();

        let new_event1 = out.recv().await;
        let new_event2 = out.recv().await;
        let new_event3 = out.recv().await;

        drop(tx);
        topology.stop().await;

        event1.set_source_id(Arc::new(ComponentKey::from("in")));
        event2.set_source_id(Arc::new(ComponentKey::from("in")));
        event3.set_source_id(Arc::new(ComponentKey::from("in")));

        event1.set_upstream_id(Arc::new(OutputId::from("transform")));
        event2.set_upstream_id(Arc::new(OutputId::from("transform")));
        event3.set_upstream_id(Arc::new(OutputId::from("transform")));

        event1.metadata_mut().set_schema_definition(&Arc::new(
            Definition::new_with_default_metadata(Kind::any_object(), [LogNamespace::Legacy]),
        ));
        event2.metadata_mut().set_schema_definition(&Arc::new(
            Definition::new_with_default_metadata(Kind::any_object(), [LogNamespace::Legacy]),
        ));
        event3.metadata_mut().set_schema_definition(&Arc::new(
            Definition::new_with_default_metadata(Kind::any_object(), [LogNamespace::Legacy]),
        ));

        assert_eq!(new_event1, Some(event1));
        assert_eq!(new_event2, Some(event2));
        // The third event should have been modified to remove "tag1"
        assert_ne!(new_event3, Some(event3));

        let new_event3 = new_event3.unwrap();
        assert!(!new_event3.as_metric().tags().unwrap().contains_key("tag1"));
        assert_eq!(
            "val1",
            new_event3.as_metric().tags().unwrap().get("tag2").unwrap()
        );
    })
    .await;
}

#[tokio::test]
async fn tag_cardinality_limit_drop_tag_hashset_multi_value() {
    drop_tag_multi_value(make_transform_hashset(2, LimitExceededAction::DropTag)).await;
}

#[tokio::test]
async fn tag_cardinality_limit_drop_tag_bloom_multi_value() {
    drop_tag_multi_value(make_transform_bloom(2, LimitExceededAction::DropTag)).await;
}

async fn drop_tag_multi_value(config: TagCardinalityLimitConfig) {
    assert_transform_compliance(async move {
        let mut tags1 = MetricTags::default();
        tags1.set_multi_value(
            "tag1".to_string(),
            vec![
                TagValue::Value("val1.a".to_string()),
                TagValue::Value("val1.b".to_string()),
            ],
        );
        let mut event1 = make_metric(tags1);

        let mut tags2 = MetricTags::default();
        tags2.set_multi_value(
            "tag1".to_string(),
            vec![
                TagValue::Value("val1.a".to_string()),
                TagValue::Value("val1.c".to_string()),
            ],
        );
        let mut event2 = make_metric(tags2);

        let mut tags3 = MetricTags::default();
        tags3.set_multi_value(
            "tag1".to_string(),
            vec![
                TagValue::Value("val1.b".to_string()),
                TagValue::Value("val1.c".to_string()),
            ],
        );
        let mut event3 = make_metric(tags3);

        let (tx, rx) = mpsc::channel(1);
        let (topology, mut out) = create_topology(ReceiverStream::new(rx), config).await;

        tx.send(event1.clone()).await.unwrap();
        tx.send(event2.clone()).await.unwrap();
        tx.send(event3.clone()).await.unwrap();

        let new_event1 = out.recv().await;
        let new_event2 = out.recv().await;
        let new_event3 = out.recv().await;

        event1.set_source_id(Arc::new(ComponentKey::from("in")));
        event2.set_source_id(Arc::new(ComponentKey::from("in")));
        event3.set_source_id(Arc::new(ComponentKey::from("in")));

        event1.set_upstream_id(Arc::new(OutputId::from("transform")));
        event2.set_upstream_id(Arc::new(OutputId::from("transform")));
        event3.set_upstream_id(Arc::new(OutputId::from("transform")));

        // definitions aren't valid for metrics yet, it's just set to the default (anything).
        event1.metadata_mut().set_schema_definition(&Arc::new(
            Definition::new_with_default_metadata(Kind::any_object(), [LogNamespace::Legacy]),
        ));
        event2.metadata_mut().set_schema_definition(&Arc::new(
            Definition::new_with_default_metadata(Kind::any_object(), [LogNamespace::Legacy]),
        ));
        event3.metadata_mut().set_schema_definition(&Arc::new(
            Definition::new_with_default_metadata(Kind::any_object(), [LogNamespace::Legacy]),
        ));

        drop(tx);
        topology.stop().await;

        assert_eq!(new_event1, Some(event1));
        assert_eq!(new_event2, Some(event2));
        // The third event should have been modified to remove "tag1"
        assert_ne!(new_event3, Some(event3));
    })
    .await;
}

#[tokio::test]
async fn tag_cardinality_limit_separate_value_limit_per_tag_hashset() {
    separate_value_limit_per_tag(make_transform_hashset(2, LimitExceededAction::DropEvent)).await;
}

#[tokio::test]
async fn tag_cardinality_limit_separate_value_limit_per_tag_bloom() {
    separate_value_limit_per_tag(make_transform_bloom(2, LimitExceededAction::DropEvent)).await;
}

/// Test that hitting the value limit on one tag does not affect the ability to take new
/// values for other tags.
async fn separate_value_limit_per_tag(config: TagCardinalityLimitConfig) {
    assert_transform_compliance(async move {
        let mut event1 = make_metric(metric_tags!("tag1" => "val1", "tag2" => "val1"));

        let mut event2 = make_metric(metric_tags!("tag1" => "val2", "tag2" => "val1"));

        // Now value limit is reached for "tag1", but "tag2" still has values available.
        let mut event3 = make_metric(metric_tags!("tag1" => "val1", "tag2" => "val2"));

        let (tx, rx) = mpsc::channel(1);
        let (topology, mut out) = create_topology(ReceiverStream::new(rx), config).await;

        tx.send(event1.clone()).await.unwrap();
        tx.send(event2.clone()).await.unwrap();
        tx.send(event3.clone()).await.unwrap();

        let new_event1 = out.recv().await;
        let new_event2 = out.recv().await;
        let new_event3 = out.recv().await;

        drop(tx);
        topology.stop().await;

        event1.set_source_id(Arc::new(ComponentKey::from("in")));
        event2.set_source_id(Arc::new(ComponentKey::from("in")));
        event3.set_source_id(Arc::new(ComponentKey::from("in")));

        event1.set_upstream_id(Arc::new(OutputId::from("transform")));
        event2.set_upstream_id(Arc::new(OutputId::from("transform")));
        event3.set_upstream_id(Arc::new(OutputId::from("transform")));

        // definitions aren't valid for metrics yet, it's just set to the default (anything).
        event1.metadata_mut().set_schema_definition(&Arc::new(
            Definition::new_with_default_metadata(Kind::any_object(), [LogNamespace::Legacy]),
        ));
        event2.metadata_mut().set_schema_definition(&Arc::new(
            Definition::new_with_default_metadata(Kind::any_object(), [LogNamespace::Legacy]),
        ));
        event3.metadata_mut().set_schema_definition(&Arc::new(
            Definition::new_with_default_metadata(Kind::any_object(), [LogNamespace::Legacy]),
        ));

        assert_eq!(new_event1, Some(event1));
        assert_eq!(new_event2, Some(event2));
        assert_eq!(new_event3, Some(event3));
    })
    .await;
}

/// Test that hitting the value limit on one tag does not affect checking the limit on other
/// tags that happen to be ordered later
#[test]
fn drop_event_checks_all_tags1() {
    drop_event_checks_all_tags(|val1, val2| metric_tags!("tag1" => val1, "tag2" => val2));
}

#[test]
fn drop_event_checks_all_tags2() {
    drop_event_checks_all_tags(|val1, val2| metric_tags!("tag1" => val2, "tag2" => val1));
}

fn drop_event_checks_all_tags(make_tags: impl Fn(&str, &str) -> MetricTags) {
    let config = make_transform_hashset(2, LimitExceededAction::DropEvent);
    let mut transform = TagCardinalityLimit::new(config);

    let event1 = make_metric(make_tags("val1", "val1"));
    let event2 = make_metric(make_tags("val2", "val1"));
    // Next the limit is exceeded for the first tag.
    let event3 = make_metric(make_tags("val3", "val2"));
    // And then check if the new value for the second tag was not recorded by the above event.
    let event4 = make_metric(make_tags("val1", "val3"));

    let new_event1 = transform.transform_one(event1.clone());
    let new_event2 = transform.transform_one(event2.clone());
    let new_event3 = transform.transform_one(event3);
    let new_event4 = transform.transform_one(event4.clone());

    assert_eq!(new_event1, Some(event1));
    assert_eq!(new_event2, Some(event2));
    assert_eq!(new_event3, None);
    assert_eq!(new_event4, Some(event4));
}

#[tokio::test]
async fn tag_cardinality_limit_separate_value_limit_per_metric_name_hashset() {
    separate_value_limit_per_metric_name(make_transform_hashset_with_per_metric_limits(
        2,
        LimitExceededAction::DropTag,
        HashMap::from([
            (
                "metricA".to_string(),
                PerMetricConfig {
                    namespace: None,
                    config: make_transform_hashset(1, LimitExceededAction::DropTag).global,
                },
            ),
            (
                "metricB".to_string(),
                PerMetricConfig {
                    namespace: None,
                    config: make_transform_hashset(5, LimitExceededAction::DropTag).global,
                },
            ),
        ]),
    ))
    .await;
}

#[tokio::test]
async fn tag_cardinality_limit_separate_value_limit_per_metric_name_bloom() {
    separate_value_limit_per_metric_name(make_transform_bloom_with_per_metric_limits(
        2,
        LimitExceededAction::DropTag,
        HashMap::from([
            (
                "metricA".to_string(),
                PerMetricConfig {
                    namespace: None,
                    config: make_transform_bloom(1, LimitExceededAction::DropTag).global,
                },
            ),
            (
                "metricB".to_string(),
                PerMetricConfig {
                    namespace: None,
                    config: make_transform_bloom(5, LimitExceededAction::DropTag).global,
                },
            ),
        ]),
    ))
    .await;
}

/// Test that hitting the value limit on one tag does not affect the ability to take new
/// values for other tags.
async fn separate_value_limit_per_metric_name(config: TagCardinalityLimitConfig) {
    assert_transform_compliance(async move {
        let mut event_a1 =
            make_metric_with_name(metric_tags!("tag1" => "val1", "tag2" => "val1"), "metricA");

        // The limit for tag1 should already be reached here
        let mut event_a2 =
            make_metric_with_name(metric_tags!("tag1" => "val2", "tag2" => "val1"), "metricA");

        // The limit for tag2 should be reached here
        let mut event_a3 =
            make_metric_with_name(metric_tags!("tag1" => "val1", "tag2" => "val2"), "metricA");

        // MetricB should have all of its tags kept due to higher limit
        let mut event_b1 =
            make_metric_with_name(metric_tags!("tag1" => "val1", "tag2" => "val1"), "metricB");
        let mut event_b2 =
            make_metric_with_name(metric_tags!("tag1" => "val2", "tag2" => "val1"), "metricB");
        let mut event_b3 =
            make_metric_with_name(metric_tags!("tag1" => "val1", "tag2" => "val2"), "metricB");

        // MetricC has no specific config, so it uses the global config, which allows 2 values
        let mut event_c1 =
            make_metric_with_name(metric_tags!("tag1" => "val1", "tag2" => "val1"), "metricC");
        let mut event_c2 =
            make_metric_with_name(metric_tags!("tag1" => "val2", "tag2" => "val2"), "metricC");
        // The limit for tag2 should be reached here
        let mut event_c3 =
            make_metric_with_name(metric_tags!("tag1" => "val1", "tag2" => "val3"), "metricC");

        let (tx, rx) = mpsc::channel(1);
        let (topology, mut out) = create_topology(ReceiverStream::new(rx), config).await;

        let events = vec![
            &mut event_a1,
            &mut event_a2,
            &mut event_a3,
            &mut event_b1,
            &mut event_b2,
            &mut event_b3,
            &mut event_c1,
            &mut event_c2,
            &mut event_c3,
        ];

        for event in &events {
            tx.send((*event).clone()).await.unwrap();
        }

        let new_event_a1 = out.recv().await;
        let new_event_a2 = out.recv().await;
        let new_event_a3 = out.recv().await;
        let new_event_b1 = out.recv().await;
        let new_event_b2 = out.recv().await;
        let new_event_b3 = out.recv().await;
        let new_event_c1 = out.recv().await;
        let new_event_c2 = out.recv().await;
        let new_event_c3 = out.recv().await;

        drop(tx);
        topology.stop().await;

        for event in events {
            event.set_source_id(Arc::new(ComponentKey::from("in")));
            event.set_upstream_id(Arc::new(OutputId::from("transform")));
            event.metadata_mut().set_schema_definition(&Arc::new(
                Definition::new_with_default_metadata(Kind::any_object(), [LogNamespace::Legacy]),
            ));
        }

        assert_eq!(new_event_a1, Some(event_a1));
        // The second event should have been modified to remove "tag1"
        let new_event_a2 = new_event_a2.unwrap();
        assert!(!new_event_a2
            .as_metric()
            .tags()
            .unwrap()
            .contains_key("tag1"));
        assert_eq!(
            "val1",
            new_event_a2
                .as_metric()
                .tags()
                .unwrap()
                .get("tag2")
                .unwrap()
        );

        // The third event should have been modified to remove "tag2"
        let new_event_a3 = new_event_a3.unwrap();
        assert!(!new_event_a3
            .as_metric()
            .tags()
            .unwrap()
            .contains_key("tag2"));
        assert_eq!(
            "val1",
            new_event_a3
                .as_metric()
                .tags()
                .unwrap()
                .get("tag1")
                .unwrap()
        );

        assert_eq!(new_event_b1, Some(event_b1));
        assert_eq!(new_event_b2, Some(event_b2));
        assert_eq!(new_event_b3, Some(event_b3));

        assert_eq!(new_event_c1, Some(event_c1));
        assert_eq!(new_event_c2, Some(event_c2));
        // The third event should have been modified to remove "tag2"
        let new_event_c3 = new_event_c3.unwrap();
        assert!(!new_event_c3
            .as_metric()
            .tags()
            .unwrap()
            .contains_key("tag2"));
        assert_eq!(
            "val1",
            new_event_c3
                .as_metric()
                .tags()
                .unwrap()
                .get("tag1")
                .unwrap()
        );
    })
    .await;
}
