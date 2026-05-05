use std::{collections::HashMap, sync::Arc};

use config::{PerMetricConfig, PerTagConfig};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use vector_lib::{
    config::{ComponentKey, OutputId},
    event::EventMetadata,
    metric_tags,
};
use vrl::compiler::prelude::Kind;

use super::*;
use crate::{
    config::{LogNamespace, schema::Definition},
    event::{Event, Metric, MetricTags, metric, metric::TagValue},
    test_util::components::assert_transform_compliance,
    transforms::{
        tag_cardinality_limit::config::{
            BloomFilterConfig, InternalMetricsConfig, Mode, default_cache_size,
        },
        test::create_topology,
    },
};

#[test]
fn generate_config() {
    crate::test_util::test_generate_config::<Config>();
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
) -> Config {
    Config {
        global: Inner {
            value_limit,
            limit_exceeded_action,
            mode: Mode::Exact,
            internal_metrics: InternalMetricsConfig::default(),
        },
        tracking_scope: TrackingScope::default(),
        per_metric_limits: HashMap::new(),
    }
}

fn make_transform_bloom(value_limit: usize, limit_exceeded_action: LimitExceededAction) -> Config {
    Config {
        global: Inner {
            value_limit,
            limit_exceeded_action,
            mode: Mode::Probabilistic(BloomFilterConfig {
                cache_size_per_key: default_cache_size(),
            }),
            internal_metrics: InternalMetricsConfig::default(),
        },
        tracking_scope: TrackingScope::default(),
        per_metric_limits: HashMap::new(),
    }
}

fn make_transform_hashset_with_per_metric_limits(
    value_limit: usize,
    limit_exceeded_action: LimitExceededAction,
    per_metric_limits: HashMap<String, PerMetricConfig>,
) -> Config {
    Config {
        global: Inner {
            value_limit,
            limit_exceeded_action,
            mode: Mode::Exact,
            internal_metrics: InternalMetricsConfig::default(),
        },
        tracking_scope: TrackingScope::default(),
        per_metric_limits,
    }
}

fn make_transform_bloom_with_per_metric_limits(
    value_limit: usize,
    limit_exceeded_action: LimitExceededAction,
    per_metric_limits: HashMap<String, PerMetricConfig>,
) -> Config {
    Config {
        global: Inner {
            value_limit,
            limit_exceeded_action,
            mode: Mode::Probabilistic(BloomFilterConfig {
                cache_size_per_key: default_cache_size(),
            }),
            internal_metrics: InternalMetricsConfig::default(),
        },
        tracking_scope: TrackingScope::default(),
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

async fn drop_event(config: Config) {
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

async fn drop_tag(config: Config) {
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

async fn drop_tag_multi_value(config: Config) {
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
async fn separate_value_limit_per_tag(config: Config) {
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

fn override_inner_hashset(value_limit: usize, action: LimitExceededAction) -> OverrideInner {
    OverrideInner {
        value_limit,
        limit_exceeded_action: action,
        mode: OverrideMode::Exact,
        internal_metrics: InternalMetricsConfig::default(),
    }
}

fn override_inner_bloom(value_limit: usize, action: LimitExceededAction) -> OverrideInner {
    OverrideInner {
        value_limit,
        limit_exceeded_action: action,
        mode: OverrideMode::Probabilistic(BloomFilterConfig {
            cache_size_per_key: default_cache_size(),
        }),
        internal_metrics: InternalMetricsConfig::default(),
    }
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
                    per_tag_limits: HashMap::new(),
                    config: override_inner_hashset(1, LimitExceededAction::DropTag),
                },
            ),
            (
                "metricB".to_string(),
                PerMetricConfig {
                    namespace: None,
                    per_tag_limits: HashMap::new(),
                    config: override_inner_hashset(5, LimitExceededAction::DropTag),
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
                    per_tag_limits: HashMap::new(),
                    config: override_inner_bloom(1, LimitExceededAction::DropTag),
                },
            ),
            (
                "metricB".to_string(),
                PerMetricConfig {
                    namespace: None,
                    per_tag_limits: HashMap::new(),
                    config: override_inner_bloom(5, LimitExceededAction::DropTag),
                },
            ),
        ]),
    ))
    .await;
}

/// Test that hitting the value limit on one tag does not affect the ability to take new
/// values for other tags.
async fn separate_value_limit_per_metric_name(config: Config) {
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
        assert!(
            !new_event_a2
                .as_metric()
                .tags()
                .unwrap()
                .contains_key("tag1")
        );
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
        assert!(
            !new_event_a3
                .as_metric()
                .tags()
                .unwrap()
                .contains_key("tag2")
        );
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
        assert!(
            !new_event_c3
                .as_metric()
                .tags()
                .unwrap()
                .contains_key("tag2")
        );
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

/// With `tracking_scope: per_metric`, two metrics without explicit `per_metric_limits` entries
/// each get their own tracking bucket, so one hitting the limit does not affect the other.
#[test]
fn tracking_scope_per_metric_isolates_metrics() {
    let mut config = make_transform_hashset(2, LimitExceededAction::DropEvent);
    config.tracking_scope = TrackingScope::PerMetric;
    let mut transform = TagCardinalityLimit::new(config);

    // Fill metric_a's bucket to its limit (2 distinct values).
    let a1 = make_metric_with_name(metric_tags!("tag" => "v1"), "metric_a");
    let a2 = make_metric_with_name(metric_tags!("tag" => "v2"), "metric_a");
    // metric_b should be tracked in its own bucket — not affected by metric_a.
    let b1 = make_metric_with_name(metric_tags!("tag" => "v3"), "metric_b");
    let b2 = make_metric_with_name(metric_tags!("tag" => "v4"), "metric_b");
    // A 3rd unique value on metric_a should be rejected (its bucket is full).
    let a3 = make_metric_with_name(metric_tags!("tag" => "v5"), "metric_a");

    assert_eq!(transform.transform_one(a1.clone()), Some(a1));
    assert_eq!(transform.transform_one(a2.clone()), Some(a2));
    assert_eq!(transform.transform_one(b1.clone()), Some(b1));
    assert_eq!(transform.transform_one(b2.clone()), Some(b2));
    assert_eq!(transform.transform_one(a3), None);
}

/// With the default `tracking_scope: global`, metrics without explicit `per_metric_limits`
/// entries share a single tracking bucket — so values from different metrics pool together.
#[test]
fn tracking_scope_global_pools_metrics() {
    // Default `tracking_scope` is `Global`.
    let config = make_transform_hashset(2, LimitExceededAction::DropEvent);
    let mut transform = TagCardinalityLimit::new(config);

    let a1 = make_metric_with_name(metric_tags!("tag" => "v1"), "metric_a");
    // Different metric, but values pool into the shared bucket → 2/2 used.
    let b1 = make_metric_with_name(metric_tags!("tag" => "v2"), "metric_b");
    // 3rd unique value across the shared bucket → rejected.
    let a2 = make_metric_with_name(metric_tags!("tag" => "v3"), "metric_a");

    assert_eq!(transform.transform_one(a1.clone()), Some(a1));
    assert_eq!(transform.transform_one(b1.clone()), Some(b1));
    assert_eq!(transform.transform_one(a2), None);
}

fn make_per_tag(value_limit: usize, mode: Mode) -> PerTagConfig {
    let mode = match mode {
        Mode::Exact => OverrideMode::Exact,
        Mode::Probabilistic(b) => OverrideMode::Probabilistic(b),
    };
    PerTagConfig {
        config: PerTagInner {
            value_limit: Some(value_limit),
            mode,
        },
    }
}

fn make_per_metric(
    value_limit: usize,
    action: LimitExceededAction,
    per_tag_limits: HashMap<String, PerTagConfig>,
) -> PerMetricConfig {
    PerMetricConfig {
        namespace: None,
        per_tag_limits,
        config: OverrideInner {
            value_limit,
            limit_exceeded_action: action,
            mode: OverrideMode::Exact,
            internal_metrics: InternalMetricsConfig::default(),
        },
    }
}

fn make_per_metric_excluded(per_tag_limits: HashMap<String, PerTagConfig>) -> PerMetricConfig {
    PerMetricConfig {
        namespace: None,
        per_tag_limits,
        config: OverrideInner {
            value_limit: 0,
            limit_exceeded_action: LimitExceededAction::DropTag,
            mode: OverrideMode::Excluded,
            internal_metrics: InternalMetricsConfig::default(),
        },
    }
}

fn make_per_tag_excluded() -> PerTagConfig {
    PerTagConfig {
        config: PerTagInner {
            value_limit: None,
            mode: OverrideMode::Excluded,
        },
    }
}

/// A per-tag `value_limit` override caps that tag below the per-metric limit while sibling
/// tags continue to use the per-metric limit.
#[test]
fn per_tag_value_limit() {
    let config = make_transform_hashset_with_per_metric_limits(
        500,
        LimitExceededAction::DropTag,
        HashMap::from([(
            "metricA".to_string(),
            make_per_metric(
                5,
                LimitExceededAction::DropTag,
                HashMap::from([("tag1".to_string(), make_per_tag(2, Mode::Exact))]),
            ),
        )]),
    );
    let mut transform = TagCardinalityLimit::new(config);

    // Fill tag1 to its per-tag limit of 2 and tag2 to 2 of its per-metric limit of 5.
    let e1 = transform
        .transform_one(make_metric_with_name(
            metric_tags!("tag1" => "v1", "tag2" => "v1"),
            "metricA",
        ))
        .unwrap();
    assert!(e1.as_metric().tags().unwrap().contains_key("tag1"));
    assert!(e1.as_metric().tags().unwrap().contains_key("tag2"));

    let e2 = transform
        .transform_one(make_metric_with_name(
            metric_tags!("tag1" => "v2", "tag2" => "v2"),
            "metricA",
        ))
        .unwrap();
    assert!(e2.as_metric().tags().unwrap().contains_key("tag1"));
    assert!(e2.as_metric().tags().unwrap().contains_key("tag2"));

    // tag1 is at its per-tag limit; new value should be dropped. tag2 still has room.
    let e3 = transform
        .transform_one(make_metric_with_name(
            metric_tags!("tag1" => "v3", "tag2" => "v3"),
            "metricA",
        ))
        .unwrap();
    assert!(!e3.as_metric().tags().unwrap().contains_key("tag1"));
    assert_eq!("v3", e3.as_metric().tags().unwrap().get("tag2").unwrap());

    // Fill tag2 to the per-metric limit of 5.
    let e4 = transform
        .transform_one(make_metric_with_name(
            metric_tags!("tag2" => "v4"),
            "metricA",
        ))
        .unwrap();
    assert_eq!("v4", e4.as_metric().tags().unwrap().get("tag2").unwrap());

    let e5 = transform
        .transform_one(make_metric_with_name(
            metric_tags!("tag2" => "v5"),
            "metricA",
        ))
        .unwrap();
    assert_eq!("v5", e5.as_metric().tags().unwrap().get("tag2").unwrap());

    // tag2 is now at its per-metric limit; new value should be dropped.
    let e6 = transform
        .transform_one(make_metric_with_name(
            metric_tags!("tag2" => "v6"),
            "metricA",
        ))
        .unwrap();
    assert!(!e6.as_metric().tags().unwrap().contains_key("tag2"));
}

/// Tags with no per-tag override fall back to the per-metric configuration; metrics with no
/// per-metric override fall back to the global configuration.
#[test]
fn per_tag_falls_back_to_per_metric() {
    let config = make_transform_hashset_with_per_metric_limits(
        2,
        LimitExceededAction::DropTag,
        HashMap::from([(
            "metricA".to_string(),
            make_per_metric(
                3,
                LimitExceededAction::DropTag,
                HashMap::from([("tag1".to_string(), make_per_tag(1, Mode::Exact))]),
            ),
        )]),
    );
    let mut transform = TagCardinalityLimit::new(config);

    // metricA: tag1 capped at 1 (per-tag), tag2 capped at 3 (per-metric).
    transform.transform_one(make_metric_with_name(
        metric_tags!("tag1" => "v1", "tag2" => "v1"),
        "metricA",
    ));
    let e2 = transform
        .transform_one(make_metric_with_name(
            metric_tags!("tag1" => "v2", "tag2" => "v2"),
            "metricA",
        ))
        .unwrap();
    // tag1 already at its per-tag limit of 1 → dropped. tag2 accepted (now 2/3).
    assert!(!e2.as_metric().tags().unwrap().contains_key("tag1"));
    assert_eq!("v2", e2.as_metric().tags().unwrap().get("tag2").unwrap());

    let e3 = transform
        .transform_one(make_metric_with_name(
            metric_tags!("tag2" => "v3"),
            "metricA",
        ))
        .unwrap();
    assert_eq!("v3", e3.as_metric().tags().unwrap().get("tag2").unwrap());

    // tag2 now at per-metric limit of 3 → dropped.
    let e4 = transform
        .transform_one(make_metric_with_name(
            metric_tags!("tag2" => "v4"),
            "metricA",
        ))
        .unwrap();
    assert!(!e4.as_metric().tags().unwrap().contains_key("tag2"));

    // metricB has no per-metric entry → uses global limit of 2.
    transform.transform_one(make_metric_with_name(
        metric_tags!("tag1" => "v1"),
        "metricB",
    ));
    transform.transform_one(make_metric_with_name(
        metric_tags!("tag1" => "v2"),
        "metricB",
    ));
    let b3 = transform
        .transform_one(make_metric_with_name(
            metric_tags!("tag1" => "v3"),
            "metricB",
        ))
        .unwrap();
    assert!(!b3.as_metric().tags().unwrap().contains_key("tag1"));
}

/// An excluded metric passes all tag values through unbounded; storage is never allocated for
/// it, and other metrics are unaffected.
#[test]
fn metric_excluded_passes_through_unbounded() {
    let config = make_transform_hashset_with_per_metric_limits(
        1,
        LimitExceededAction::DropTag,
        HashMap::from([(
            "metricA".to_string(),
            make_per_metric_excluded(HashMap::new()),
        )]),
    );
    let mut transform = TagCardinalityLimit::new(config);

    // Send 100 distinct values for metricA's tag — all should pass through.
    for i in 0..100 {
        let v = format!("v{i}");
        let e = transform
            .transform_one(make_metric_with_name(
                metric_tags!("tag1" => v.clone()),
                "metricA",
            ))
            .unwrap();
        assert_eq!(
            v.as_str(),
            e.as_metric().tags().unwrap().get("tag1").unwrap()
        );
    }
    // Excluded metric must not have allocated any storage.
    assert!(
        transform
            .accepted_tags
            .get(&Some((None, "metricA".to_string())))
            .is_none()
    );

    // metricB has no per-metric override → uses global limit of 1.
    transform.transform_one(make_metric_with_name(
        metric_tags!("tag1" => "v1"),
        "metricB",
    ));
    let e = transform
        .transform_one(make_metric_with_name(
            metric_tags!("tag1" => "v2"),
            "metricB",
        ))
        .unwrap();
    assert!(!e.as_metric().tags().unwrap().contains_key("tag1"));
}

/// An excluded tag is unbounded; sibling tags on the same metric remain limited.
#[test]
fn tag_excluded_unbounded_sibling_limited() {
    let config = make_transform_hashset_with_per_metric_limits(
        500,
        LimitExceededAction::DropTag,
        HashMap::from([(
            "metricA".to_string(),
            make_per_metric(
                2,
                LimitExceededAction::DropTag,
                HashMap::from([("trace_id".to_string(), make_per_tag_excluded())]),
            ),
        )]),
    );
    let mut transform = TagCardinalityLimit::new(config);

    // 100 distinct trace_id values pass; host capped at 2.
    for i in 0..100 {
        let trace = format!("t{i}");
        let host = format!("h{}", i % 5);
        let e = transform
            .transform_one(make_metric_with_name(
                metric_tags!("trace_id" => trace.clone(), "host" => host.clone()),
                "metricA",
            ))
            .unwrap();
        assert_eq!(
            trace.as_str(),
            e.as_metric().tags().unwrap().get("trace_id").unwrap(),
            "trace_id should always be retained (excluded)"
        );
        if i >= 2 && (host != "h0" && host != "h1") {
            assert!(
                !e.as_metric().tags().unwrap().contains_key("host"),
                "host {host} beyond limit should be dropped"
            );
        }
    }
}

/// A per-tag entry with `value_limit` unset must inherit the per-metric `value_limit`
/// rather than silently falling back to the serde default of 500.
#[test]
fn per_tag_value_limit_inherits_from_per_metric() {
    let per_tag = PerTagConfig {
        config: PerTagInner {
            value_limit: None, // unset → should inherit from the per-metric (3)
            mode: OverrideMode::Exact,
        },
    };
    let config = make_transform_hashset_with_per_metric_limits(
        500,
        LimitExceededAction::DropTag,
        HashMap::from([(
            "metricA".to_string(),
            make_per_metric(
                3,
                LimitExceededAction::DropTag,
                HashMap::from([("tag1".to_string(), per_tag)]),
            ),
        )]),
    );
    let mut transform = TagCardinalityLimit::new(config);

    // First 3 distinct values for tag1 are accepted (inherits per-metric limit of 3).
    for i in 0..3 {
        let v = format!("v{i}");
        let e = transform
            .transform_one(make_metric_with_name(
                metric_tags!("tag1" => v.clone()),
                "metricA",
            ))
            .unwrap();
        assert_eq!(
            v.as_str(),
            e.as_metric().tags().unwrap().get("tag1").unwrap()
        );
    }

    // 4th value should be rejected — proves the per-tag entry did NOT silently widen
    // the limit to 500.
    let e4 = transform
        .transform_one(make_metric_with_name(
            metric_tags!("tag1" => "v3"),
            "metricA",
        ))
        .unwrap();
    assert!(!e4.as_metric().tags().unwrap().contains_key("tag1"));
}
