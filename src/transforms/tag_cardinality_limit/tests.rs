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
            default_ttl_generations,
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

/// Default `Inner` for tests: no TTL, default generations. Used as a base for
/// the `make_transform_*` helpers and any test that constructs `Inner` literally.
fn default_inner(value_limit: usize, action: LimitExceededAction, mode: Mode) -> Inner {
    Inner {
        value_limit,
        limit_exceeded_action: action,
        mode,
        internal_metrics: InternalMetricsConfig::default(),
        ttl_secs: None,
        ttl_generations: default_ttl_generations(),
    }
}

fn make_transform_hashset(
    value_limit: usize,
    limit_exceeded_action: LimitExceededAction,
) -> Config {
    Config {
        global: default_inner(value_limit, limit_exceeded_action, Mode::Exact),
        tracking_scope: TrackingScope::default(),
        max_tracked_keys: None,
        per_metric_limits: HashMap::new(),
        per_tag_limits: HashMap::new(),
    }
}

fn make_transform_bloom(value_limit: usize, limit_exceeded_action: LimitExceededAction) -> Config {
    Config {
        global: default_inner(
            value_limit,
            limit_exceeded_action,
            Mode::Probabilistic(BloomFilterConfig {
                cache_size_per_key: default_cache_size(),
            }),
        ),
        tracking_scope: TrackingScope::default(),
        max_tracked_keys: None,
        per_metric_limits: HashMap::new(),
        per_tag_limits: HashMap::new(),
    }
}

fn make_transform_hashset_with_per_metric_limits(
    value_limit: usize,
    limit_exceeded_action: LimitExceededAction,
    per_metric_limits: HashMap<String, PerMetricConfig>,
) -> Config {
    Config {
        global: default_inner(value_limit, limit_exceeded_action, Mode::Exact),
        tracking_scope: TrackingScope::default(),
        max_tracked_keys: None,
        per_metric_limits,
        per_tag_limits: HashMap::new(),
    }
}

fn make_transform_bloom_with_per_metric_limits(
    value_limit: usize,
    limit_exceeded_action: LimitExceededAction,
    per_metric_limits: HashMap<String, PerMetricConfig>,
) -> Config {
    Config {
        global: default_inner(
            value_limit,
            limit_exceeded_action,
            Mode::Probabilistic(BloomFilterConfig {
                cache_size_per_key: default_cache_size(),
            }),
        ),
        tracking_scope: TrackingScope::default(),
        max_tracked_keys: None,
        per_metric_limits,
        per_tag_limits: HashMap::new(),
    }
}

fn make_transform_with_global_per_tag_limits(
    value_limit: usize,
    limit_exceeded_action: LimitExceededAction,
    mode: Mode,
    per_tag_limits: HashMap<String, PerTagConfig>,
) -> Config {
    Config {
        global: default_inner(value_limit, limit_exceeded_action, mode),
        tracking_scope: TrackingScope::default(),
        max_tracked_keys: None,
        per_metric_limits: HashMap::new(),
        per_tag_limits,
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
        ttl_secs: None,
        ttl_generations: default_ttl_generations(),
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
        ttl_secs: None,
        ttl_generations: default_ttl_generations(),
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

/// With `max_tracked_keys: 2`, only 2 distinct (metric, tag-key) pairs get tracking
/// buckets. Tag values for additional pairs pass through unchecked rather than being
/// rejected. The tag values for tracked pairs are still subject to `value_limit`.
#[test]
fn max_tracked_keys_caps_pair_allocation() {
    let mut config = make_transform_hashset(1, LimitExceededAction::DropTag);
    config.max_tracked_keys = Some(2);
    let mut transform = TagCardinalityLimit::new(config);

    // First 2 distinct (metric, tag-key) pairs allocate buckets.
    // (None, "tag1") — bucket 1
    let e1 = transform
        .transform_one(make_metric(metric_tags!("tag1" => "v1")))
        .unwrap();
    assert_eq!("v1", e1.as_metric().tags().unwrap().get("tag1").unwrap());

    // (None, "tag2") — bucket 2
    let e2 = transform
        .transform_one(make_metric(metric_tags!("tag2" => "v1")))
        .unwrap();
    assert_eq!("v1", e2.as_metric().tags().unwrap().get("tag2").unwrap());

    // tag1 value_limit hit at 1 → second value rejected (drop_tag).
    let e3 = transform
        .transform_one(make_metric(metric_tags!("tag1" => "v2")))
        .unwrap();
    assert!(!e3.as_metric().tags().unwrap().contains_key("tag1"));

    // (None, "tag3") would need a 3rd bucket — cap is 2, so it passes through
    // unchecked. The tag is retained even though we can't enforce a value_limit on it.
    let e4 = transform
        .transform_one(make_metric(metric_tags!("tag3" => "v1")))
        .unwrap();
    assert_eq!("v1", e4.as_metric().tags().unwrap().get("tag3").unwrap());
    let e5 = transform
        .transform_one(make_metric(metric_tags!("tag3" => "v2")))
        .unwrap();
    assert_eq!("v2", e5.as_metric().tags().unwrap().get("tag3").unwrap());
}

/// With `max_tracked_keys` unset (default), cardinality limiting works normally and
/// new pairs are always tracked.
#[test]
fn max_tracked_keys_unlimited_by_default() {
    let config = make_transform_hashset(1, LimitExceededAction::DropTag);
    assert!(config.max_tracked_keys.is_none());
    let mut transform = TagCardinalityLimit::new(config);

    // Many distinct tag keys all get tracked; each enforces value_limit=1.
    for i in 0..10 {
        let key = format!("tag{i}");
        let _ = transform
            .transform_one(make_metric(metric_tags!(key.clone() => "v1")))
            .unwrap();
        // Second value for the same tag key should be rejected.
        let e = transform
            .transform_one(make_metric(metric_tags!(key.clone() => "v2")))
            .unwrap();
        assert!(!e.as_metric().tags().unwrap().contains_key(key.as_str()));
    }
}

/// Regression: with `value_limit: 0`, `limit_exceeded_action: drop_event`, and
/// `max_tracked_keys` exhausted, the documented untracked-passthrough behavior must
/// still apply. `tag_limit_exceeded` previously rejected *any* missing-bucket lookup
/// when `value_limit == 0`, causing events to be dropped before `record_tag_value`
/// could detect the allocation cap. New (metric, tag-key) pairs that cannot be
/// allocated must instead pass through unchecked.
#[test]
fn max_tracked_keys_passthrough_with_zero_value_limit_drop_event() {
    // metric_a uses a normal per-metric override with room for a value, so the first
    // event reserves the only allocation slot. metric_b inherits the global config,
    // which combines `value_limit: 0` + `drop_event` — the corner case that originally
    // dropped events even when the pair couldn't be tracked.
    let config = make_transform_hashset_with_per_metric_limits(
        0,
        LimitExceededAction::DropEvent,
        HashMap::from([(
            "metric_a".to_string(),
            make_per_metric(5, LimitExceededAction::DropEvent, HashMap::new()),
        )]),
    );
    let mut transform = {
        let mut c = config;
        c.tracking_scope = TrackingScope::PerMetric;
        c.max_tracked_keys = Some(1);
        TagCardinalityLimit::new(c)
    };

    // metric_a consumes the only allocation slot (its per-metric value_limit is 5).
    let a1 = make_metric_with_name(metric_tags!("tag" => "v1"), "metric_a");
    assert_eq!(transform.transform_one(a1.clone()), Some(a1));

    // metric_b inherits the global config: value_limit=0, drop_event. A new
    // (metric_b, "tag") pair would need a fresh bucket, but max_tracked_keys is
    // already at the cap. The event MUST pass through unchecked rather than being
    // dropped by the value_limit=0 guard.
    let untracked = make_metric_with_name(metric_tags!("tag" => "v2"), "metric_b");
    assert_eq!(
        transform.transform_one(untracked.clone()),
        Some(untracked),
        "untracked pair beyond max_tracked_keys must not trigger DropEvent under value_limit=0"
    );

    // A second distinct value for the same untracked pair also passes through:
    // no bucket exists, no enforcement applies.
    let untracked2 = make_metric_with_name(metric_tags!("tag" => "v3"), "metric_b");
    assert_eq!(
        transform.transform_one(untracked2.clone()),
        Some(untracked2)
    );

    // metric_b never allocated any storage.
    assert!(
        transform
            .accepted_tags
            .get(&Some((None, "metric_b".to_string())))
            .is_none(),
        "untracked pair must not occupy a tracking bucket"
    );
}

/// With `tracking_scope: per_metric`, the cap is enforced across *all* per-metric
/// buckets — not per bucket. Once the cap is hit, further metrics pass through
/// unchecked.
#[test]
fn max_tracked_keys_caps_across_per_metric_buckets() {
    let mut config = make_transform_hashset(1, LimitExceededAction::DropTag);
    config.tracking_scope = TrackingScope::PerMetric;
    config.max_tracked_keys = Some(2);
    let mut transform = TagCardinalityLimit::new(config);

    // metric_a, tag "k" → pair 1 of 2
    let _ = transform
        .transform_one(make_metric_with_name(metric_tags!("k" => "v1"), "metric_a"))
        .unwrap();
    // metric_b, tag "k" → pair 2 of 2 (different bucket because per_metric scope)
    let _ = transform
        .transform_one(make_metric_with_name(metric_tags!("k" => "v1"), "metric_b"))
        .unwrap();
    // metric_c, tag "k" → would be pair 3, cap is 2 → untracked passthrough.
    let e = transform
        .transform_one(make_metric_with_name(metric_tags!("k" => "v1"), "metric_c"))
        .unwrap();
    assert_eq!("v1", e.as_metric().tags().unwrap().get("k").unwrap());
    // Multiple distinct values on metric_c also pass through (no enforcement).
    let e = transform
        .transform_one(make_metric_with_name(metric_tags!("k" => "v2"), "metric_c"))
        .unwrap();
    assert_eq!("v2", e.as_metric().tags().unwrap().get("k").unwrap());
}

fn make_per_tag(value_limit: usize) -> PerTagConfig {
    PerTagConfig {
        mode: PerTagMode::LimitOverride { value_limit },
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
            ttl_secs: None,
            ttl_generations: default_ttl_generations(),
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
            ttl_secs: None,
            ttl_generations: default_ttl_generations(),
        },
    }
}

fn make_per_tag_excluded() -> PerTagConfig {
    PerTagConfig {
        mode: PerTagMode::Excluded,
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
                HashMap::from([("tag1".to_string(), make_per_tag(2))]),
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
                HashMap::from([("tag1".to_string(), make_per_tag(1))]),
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

/// A per-tag `LimitOverride` entry caps that tag at its explicit `value_limit`,
/// independent of the per-metric limit.
#[test]
fn per_tag_limit_override_caps_at_explicit_value() {
    let per_tag = PerTagConfig {
        mode: PerTagMode::LimitOverride { value_limit: 2 },
    };
    let config = make_transform_hashset_with_per_metric_limits(
        500,
        LimitExceededAction::DropTag,
        HashMap::from([(
            "metricA".to_string(),
            make_per_metric(
                10,
                LimitExceededAction::DropTag,
                HashMap::from([("tag1".to_string(), per_tag)]),
            ),
        )]),
    );
    let mut transform = TagCardinalityLimit::new(config);

    // First 2 values pass (per-tag limit = 2).
    for v in ["v0", "v1"] {
        let e = transform
            .transform_one(make_metric_with_name(metric_tags!("tag1" => v), "metricA"))
            .unwrap();
        assert_eq!(v, e.as_metric().tags().unwrap().get("tag1").unwrap());
    }

    // 3rd value dropped — proves the per-tag limit (2) applies, not the per-metric (10).
    let e3 = transform
        .transform_one(make_metric_with_name(
            metric_tags!("tag1" => "v2"),
            "metricA",
        ))
        .unwrap();
    assert!(!e3.as_metric().tags().unwrap().contains_key("tag1"));
}

#[test]
fn per_tag_zero_limit_drop_event_drops_first_event() {
    let config = make_transform_hashset_with_per_metric_limits(
        500,
        LimitExceededAction::DropTag,
        HashMap::from([(
            "metricA".to_string(),
            make_per_metric(
                10,
                LimitExceededAction::DropEvent,
                HashMap::from([("tag1".to_string(), make_per_tag(0))]),
            ),
        )]),
    );
    let mut transform = TagCardinalityLimit::new(config);

    let dropped = transform.transform_one(make_metric_with_name(
        metric_tags!("tag1" => "v0"),
        "metricA",
    ));
    assert_eq!(dropped, None);

    let passed = transform.transform_one(make_metric_with_name(
        metric_tags!("tag1" => "v0"),
        "metricB",
    ));
    assert!(passed.is_some());
}

/// Per-tag YAML syntax: `mode: limit_override` with `value_limit`, and `mode: excluded`.
#[test]
fn per_tag_modes_deserialize() {
    let yaml = r#"
value_limit: 5
mode: exact
per_metric_limits:
  metric_a:
    mode: exact
    per_tag_limits:
      capped_tag:
        mode: limit_override
        value_limit: 10
      excluded_tag:
        mode: excluded
"#;
    let parsed: Config = serde_yaml::from_str(yaml).expect("yaml should deserialize");
    let per_metric = parsed.per_metric_limits.get("metric_a").unwrap();

    let capped = per_metric.per_tag_limits.get("capped_tag").unwrap();
    assert_eq!(capped.mode, PerTagMode::LimitOverride { value_limit: 10 });

    let excluded = per_metric.per_tag_limits.get("excluded_tag").unwrap();
    assert_eq!(excluded.mode, PerTagMode::Excluded);
}

// ============================================================================
// Global per_tag_limits tests (top-level `Config::per_tag_limits`)
//
// Mirror the per-metric per-tag suite: covers `excluded` and `limit_override`
// at the global level, fallback semantics, and YAML deserialization.
// ============================================================================

#[test]
fn global_per_tag_excluded_drop_tag_passthrough_hashset() {
    global_per_tag_excluded_drop_tag_passthrough(Mode::Exact);
}

#[test]
fn global_per_tag_excluded_drop_tag_passthrough_bloom() {
    global_per_tag_excluded_drop_tag_passthrough(Mode::Probabilistic(BloomFilterConfig {
        cache_size_per_key: default_cache_size(),
    }));
}

/// A globally-excluded tag passes through unchanged on every metric, even when its values
/// would have exceeded `value_limit`. Sibling non-excluded tags still respect the limit.
fn global_per_tag_excluded_drop_tag_passthrough(mode: Mode) {
    let config = make_transform_with_global_per_tag_limits(
        2,
        LimitExceededAction::DropTag,
        mode,
        HashMap::from([("kube_pod_name".to_string(), make_per_tag_excluded())]),
    );
    let mut transform = TagCardinalityLimit::new(config);

    let event1 = make_metric(metric_tags!("kube_pod_name" => "pod-a", "tag1" => "val1"));
    let event2 = make_metric(metric_tags!("kube_pod_name" => "pod-b", "tag1" => "val2"));
    // value_limit=2 is hit on tag1, but kube_pod_name keeps passing through.
    let event3 = make_metric(metric_tags!("kube_pod_name" => "pod-c", "tag1" => "val3"));

    let new_event1 = transform.transform_one(event1).unwrap();
    let new_event2 = transform.transform_one(event2).unwrap();
    let new_event3 = transform.transform_one(event3).unwrap();

    for ev in [&new_event1, &new_event2, &new_event3] {
        assert!(
            ev.as_metric().tags().unwrap().contains_key("kube_pod_name"),
            "excluded tag should always pass through"
        );
    }
    assert_eq!(
        "val1",
        new_event1.as_metric().tags().unwrap().get("tag1").unwrap()
    );
    assert_eq!(
        "val2",
        new_event2.as_metric().tags().unwrap().get("tag1").unwrap()
    );
    assert!(
        !new_event3.as_metric().tags().unwrap().contains_key("tag1"),
        "non-excluded tag should still be subject to the limit"
    );
}

#[test]
fn global_per_tag_excluded_drop_event_passthrough_hashset() {
    global_per_tag_excluded_drop_event_passthrough(Mode::Exact);
}

#[test]
fn global_per_tag_excluded_drop_event_passthrough_bloom() {
    global_per_tag_excluded_drop_event_passthrough(Mode::Probabilistic(BloomFilterConfig {
        cache_size_per_key: default_cache_size(),
    }));
}

/// Under `DropEvent`, a globally-excluded tag never triggers a drop, but a non-excluded
/// tag exceeding `value_limit` still does.
fn global_per_tag_excluded_drop_event_passthrough(mode: Mode) {
    let config = make_transform_with_global_per_tag_limits(
        2,
        LimitExceededAction::DropEvent,
        mode,
        HashMap::from([("kube_pod_name".to_string(), make_per_tag_excluded())]),
    );
    let mut transform = TagCardinalityLimit::new(config);

    let event1 = make_metric(metric_tags!("kube_pod_name" => "pod-a", "tag1" => "val1"));
    let event2 = make_metric(metric_tags!("kube_pod_name" => "pod-b", "tag1" => "val2"));
    // 3rd value on non-excluded tag1 → DropEvent.
    let event3 = make_metric(metric_tags!("kube_pod_name" => "pod-c", "tag1" => "val3"));
    // tag1 reuses an accepted value, so a new pod name alone must not drop the event.
    let event4 = make_metric(metric_tags!("kube_pod_name" => "pod-d", "tag1" => "val1"));

    assert_eq!(transform.transform_one(event1.clone()), Some(event1));
    assert_eq!(transform.transform_one(event2.clone()), Some(event2));
    assert_eq!(transform.transform_one(event3), None);
    assert_eq!(transform.transform_one(event4.clone()), Some(event4));
}

/// A globally-excluded tag must never enter the cache, even after seeing many distinct
/// values. Asserting directly on `accepted_tags` pins the "never allocate" contract.
#[test]
fn global_per_tag_excluded_never_populates_cache() {
    let config = make_transform_with_global_per_tag_limits(
        2,
        LimitExceededAction::DropTag,
        Mode::Exact,
        HashMap::from([("kube_pod_name".to_string(), make_per_tag_excluded())]),
    );
    let mut transform = TagCardinalityLimit::new(config);

    for i in 0..10 {
        let event = make_metric(metric_tags!(
            "kube_pod_name" => format!("pod-{i}").as_str(),
            "tag1" => "val1"
        ));
        transform.transform_one(event).unwrap();
    }

    let bucket = transform
        .accepted_tags
        .get(&None)
        .expect("non-excluded tag1 should still allocate a global bucket");
    assert!(
        bucket.contains_key("tag1"),
        "non-excluded tag must still be tracked"
    );
    assert!(
        !bucket.contains_key("kube_pod_name"),
        "excluded tag key must never enter the cache"
    );
}

/// A global `LimitOverride` caps that tag at its own `value_limit` even though the global
/// `value_limit` is much higher. Other tags continue to use the global limit.
#[test]
fn global_per_tag_limit_override_caps_at_explicit_value() {
    let config = make_transform_with_global_per_tag_limits(
        500,
        LimitExceededAction::DropTag,
        Mode::Exact,
        HashMap::from([("tag1".to_string(), make_per_tag(2))]),
    );
    let mut transform = TagCardinalityLimit::new(config);

    // First 2 values pass (per-tag limit = 2).
    for v in ["v0", "v1"] {
        let e = transform
            .transform_one(make_metric(metric_tags!("tag1" => v)))
            .unwrap();
        assert_eq!(v, e.as_metric().tags().unwrap().get("tag1").unwrap());
    }

    // 3rd value dropped — proves the per-tag limit (2) applies, not the global (500).
    let e3 = transform
        .transform_one(make_metric(metric_tags!("tag1" => "v2")))
        .unwrap();
    assert!(!e3.as_metric().tags().unwrap().contains_key("tag1"));

    // tag2 has no override → still uses the global limit (500), so we can push a new value.
    let e_other = transform
        .transform_one(make_metric(metric_tags!("tag2" => "v0")))
        .unwrap();
    assert_eq!(
        "v0",
        e_other.as_metric().tags().unwrap().get("tag2").unwrap()
    );
}

/// Per-metric `per_tag_limits` shadows the global `per_tag_limits` for matched metrics:
/// when a metric has its own `per_metric_limits` entry, the global per-tag overrides are
/// not consulted for that metric. Metrics without a per-metric entry continue to use the
/// global per-tag overrides.
#[test]
fn global_per_tag_overridden_by_per_metric_entry() {
    let config = Config {
        global: default_inner(2, LimitExceededAction::DropTag, Mode::Exact),
        tracking_scope: TrackingScope::default(),
        max_tracked_keys: None,
        per_metric_limits: HashMap::from([(
            "metricA".to_string(),
            make_per_metric(5, LimitExceededAction::DropTag, HashMap::new()),
        )]),
        per_tag_limits: HashMap::from([("tag1".to_string(), make_per_tag_excluded())]),
    };
    let mut transform = TagCardinalityLimit::new(config);

    // metricA matches per_metric_limits → global per_tag_limits is ignored.
    // tag1 must therefore be tracked under metricA's per-metric `value_limit: 5`.
    for v in ["a0", "a1", "a2", "a3", "a4"] {
        let e = transform
            .transform_one(make_metric_with_name(metric_tags!("tag1" => v), "metricA"))
            .unwrap();
        assert_eq!(v, e.as_metric().tags().unwrap().get("tag1").unwrap());
    }
    // 6th value on metricA's tag1 must be dropped: per-metric limit reached.
    let dropped = transform
        .transform_one(make_metric_with_name(
            metric_tags!("tag1" => "a5"),
            "metricA",
        ))
        .unwrap();
    assert!(
        !dropped.as_metric().tags().unwrap().contains_key("tag1"),
        "per-metric per_tag_limits is empty → metricA falls back to per-metric value_limit, \
         not the global excluded entry"
    );

    // metricB has no per-metric entry → global `tag1: excluded` applies, so values pass
    // through unbounded even though the global value_limit is 2.
    for v in ["b0", "b1", "b2", "b3", "b4"] {
        let e = transform
            .transform_one(make_metric_with_name(metric_tags!("tag1" => v), "metricB"))
            .unwrap();
        assert_eq!(
            v,
            e.as_metric().tags().unwrap().get("tag1").unwrap(),
            "globally-excluded tag should pass through on unmatched metrics"
        );
    }
}

// Transform-level TTL coverage focuses on the *config surface* (defaults,
// deserialization, the public `contains_no_refresh` contract). The behavioral
// TTL tests, where we need to drive `Instant`s, live in `tag_value_set.rs`.

#[test]
fn ttl_defaults_off() {
    let cfg = make_transform_hashset(2, LimitExceededAction::DropTag);
    assert!(
        cfg.global.ttl_secs.is_none(),
        "default config must not enable TTL"
    );
    assert_eq!(
        cfg.global.ttl_generations,
        default_ttl_generations(),
        "default generations should match the documented default"
    );
}

#[test]
fn ttl_global_yaml_deserializes() {
    let yaml = r#"
value_limit: 5
mode: exact
ttl_secs: 3600
ttl_generations: 6
"#;
    let parsed: Config = serde_yaml::from_str(yaml).expect("yaml should deserialize");
    assert_eq!(parsed.global.ttl_secs, Some(3600));
    assert_eq!(parsed.global.ttl_generations, 6);
}

#[test]
fn ttl_per_metric_yaml_deserializes() {
    let yaml = r#"
value_limit: 5
mode: exact
ttl_secs: 3600
per_metric_limits:
  hot_metric:
    mode: probabilistic
    cache_size_per_key: 1024
    value_limit: 100
    ttl_secs: 600
    ttl_generations: 3
"#;
    let parsed: Config = serde_yaml::from_str(yaml).expect("yaml should deserialize");
    assert_eq!(parsed.global.ttl_secs, Some(3600));
    let pm = parsed.per_metric_limits.get("hot_metric").unwrap();
    assert_eq!(pm.config.ttl_secs, Some(600));
    assert_eq!(pm.config.ttl_generations, 3);
}

/// Pins the basic contract of `contains_no_refresh`: it must return `true`
/// for a value that was just inserted, across every backend variant. The
/// "no-refresh" timing semantic (the actual *DropEvent* contract) is verified
/// in `tag_value_set.rs::tests::{ttl_exact,rolling_bloom}_contains_no_refresh_*`,
/// where the `Instant`-driven storage methods can be exercised directly.
///
/// Note: this test does NOT verify that `tag_limit_exceeded` calls
/// `contains_no_refresh` (and not `contains`) — that wiring is enforced by
/// code review of the (private) match arm in `mod.rs::tag_limit_exceeded`.
#[test]
fn contains_no_refresh_finds_inserted_values_on_all_backends() {
    use super::tag_value_set::AcceptedTagValueSet;
    use crate::event::metric::TagValueSet;

    let v1 = TagValueSet::from(["v1".to_string()]);
    let bloom_mode = Mode::Probabilistic(BloomFilterConfig {
        cache_size_per_key: default_cache_size(),
    });

    for (label, mut set) in [
        (
            "exact no-ttl",
            AcceptedTagValueSet::new(4, &Mode::Exact, None, 4),
        ),
        (
            "bloom no-ttl",
            AcceptedTagValueSet::new(4, &bloom_mode, None, 4),
        ),
        (
            "exact ttl",
            AcceptedTagValueSet::new(4, &Mode::Exact, Some(60), 4),
        ),
        (
            "bloom ttl",
            AcceptedTagValueSet::new(4, &bloom_mode, Some(60), 4),
        ),
    ] {
        set.insert(v1.clone());
        assert!(
            set.contains_no_refresh(&v1),
            "{label}: should find inserted value"
        );
    }
}

/// `ttl_secs: 0` must select the **non-TTL** backend (same as `None`). If we
/// ever flipped this to "expire immediately" — i.e. a TTL backend with
/// `Duration::ZERO` — the 1-second `sweep_interval` floor would mask the bug
/// in any externally-observable behavior, but the cache would still degrade
/// silently the moment a sweep boundary was crossed.
#[test]
fn ttl_zero_disables_ttl() {
    use super::tag_value_set::AcceptedTagValueSet;

    let bloom_mode = Mode::Probabilistic(BloomFilterConfig {
        cache_size_per_key: default_cache_size(),
    });
    for (label, set) in [
        (
            "exact ttl=0",
            AcceptedTagValueSet::new(4, &Mode::Exact, Some(0), 4),
        ),
        (
            "bloom ttl=0",
            AcceptedTagValueSet::new(4, &bloom_mode, Some(0), 4),
        ),
        (
            "exact ttl=None",
            AcceptedTagValueSet::new(4, &Mode::Exact, None, 4),
        ),
        (
            "bloom ttl=None",
            AcceptedTagValueSet::new(4, &bloom_mode, None, 4),
        ),
    ] {
        assert!(
            !set.ttl_enabled(),
            "{label}: must select the non-TTL backend"
        );
    }

    // Sanity: TTL with a positive value DOES select the TTL backend.
    let ttl_set = AcceptedTagValueSet::new(4, &Mode::Exact, Some(60), 4);
    assert!(ttl_set.ttl_enabled(), "ttl=Some(60) should enable TTL");
}

#[test]
fn ttl_existing_yaml_unchanged() {
    // A pre-TTL config must continue to parse without any TTL fields and
    // produce ttl_secs=None — that's the backwards-compat contract.
    let yaml = r#"
value_limit: 5
mode: probabilistic
cache_size_per_key: 2048
"#;
    let parsed: Config = serde_yaml::from_str(yaml).expect("yaml should deserialize");
    assert!(parsed.global.ttl_secs.is_none());
    assert_eq!(parsed.global.ttl_generations, default_ttl_generations());
}

/// Global per-tag YAML syntax mirrors per-metric `per_tag_limits`: `mode: limit_override`
/// with `value_limit`, and `mode: excluded`.
#[test]
fn global_per_tag_modes_deserialize() {
    let yaml = r#"
value_limit: 5
mode: exact
per_tag_limits:
  capped_tag:
    mode: limit_override
    value_limit: 10
  excluded_tag:
    mode: excluded
"#;
    let parsed: Config = serde_yaml::from_str(yaml).expect("yaml should deserialize");

    let capped = parsed.per_tag_limits.get("capped_tag").unwrap();
    assert_eq!(capped.mode, PerTagMode::LimitOverride { value_limit: 10 });

    let excluded = parsed.per_tag_limits.get("excluded_tag").unwrap();
    assert_eq!(excluded.mode, PerTagMode::Excluded);
}
