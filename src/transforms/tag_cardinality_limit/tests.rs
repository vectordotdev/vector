use std::{collections::HashMap, sync::Arc};

use config::PerMetricConfig;
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
            exclude_tags: Vec::new(),
        },
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
            exclude_tags: Vec::new(),
        },
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
            exclude_tags: Vec::new(),
        },
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
            exclude_tags: Vec::new(),
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

fn bloom_mode() -> Mode {
    Mode::Probabilistic(BloomFilterConfig {
        cache_size_per_key: default_cache_size(),
    })
}

fn make_transform_with_exclude_tags(
    value_limit: usize,
    limit_exceeded_action: LimitExceededAction,
    mode: Mode,
    exclude_tags: Vec<String>,
) -> Config {
    Config {
        global: Inner {
            value_limit,
            limit_exceeded_action,
            mode,
            internal_metrics: InternalMetricsConfig::default(),
            exclude_tags,
        },
        per_metric_limits: HashMap::new(),
    }
}

#[test]
fn exclude_tags_drop_tag_passthrough_hashset() {
    exclude_tags_drop_tag_passthrough(Mode::Exact);
}

#[test]
fn exclude_tags_drop_tag_passthrough_bloom() {
    exclude_tags_drop_tag_passthrough(bloom_mode());
}

/// Excluded tag keys pass through even when their values would have exceeded `value_limit`,
/// while non-excluded tags on the same metric still respect the limit.
fn exclude_tags_drop_tag_passthrough(mode: Mode) {
    let config = make_transform_with_exclude_tags(
        2,
        LimitExceededAction::DropTag,
        mode,
        vec!["kube_pod_name".to_string()],
    );
    let mut transform = TagCardinalityLimit::new(config);

    let event1 = make_metric(metric_tags!("kube_pod_name" => "pod-a", "tag1" => "val1"));
    let event2 = make_metric(metric_tags!("kube_pod_name" => "pod-b", "tag1" => "val2"));
    // Third distinct value for both tags; only tag1 should be dropped.
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
fn exclude_tags_drop_event_passthrough_hashset() {
    exclude_tags_drop_event_passthrough(Mode::Exact);
}

#[test]
fn exclude_tags_drop_event_passthrough_bloom() {
    exclude_tags_drop_event_passthrough(bloom_mode());
}

/// Excluded tag keys do not cause an event to be dropped under `DropEvent`, even when their
/// values would have exceeded `value_limit`.
fn exclude_tags_drop_event_passthrough(mode: Mode) {
    let config = make_transform_with_exclude_tags(
        2,
        LimitExceededAction::DropEvent,
        mode,
        vec!["kube_pod_name".to_string()],
    );
    let mut transform = TagCardinalityLimit::new(config);

    let event1 = make_metric(metric_tags!("kube_pod_name" => "pod-a", "tag1" => "val1"));
    let event2 = make_metric(metric_tags!("kube_pod_name" => "pod-b", "tag1" => "val2"));
    // val3 on a non-excluded tag still triggers DropEvent.
    let event3 = make_metric(metric_tags!("kube_pod_name" => "pod-c", "tag1" => "val3"));
    // tag1 reuses an accepted value, so a new pod name alone must not drop the event.
    let event4 = make_metric(metric_tags!("kube_pod_name" => "pod-d", "tag1" => "val1"));

    assert_eq!(transform.transform_one(event1.clone()), Some(event1));
    assert_eq!(transform.transform_one(event2.clone()), Some(event2));
    assert_eq!(transform.transform_one(event3), None);
    assert_eq!(transform.transform_one(event4.clone()), Some(event4));
}

#[test]
fn exclude_tags_merge_global_and_per_metric_hashset() {
    exclude_tags_merge_global_and_per_metric(Mode::Exact);
}

#[test]
fn exclude_tags_merge_global_and_per_metric_bloom() {
    exclude_tags_merge_global_and_per_metric(bloom_mode());
}

/// Global `exclude_tags` still applies to metrics that match a per-metric configuration —
/// the effective list is the union of global and per-metric exclusions. Parameterised over
/// `Mode` so the contract is pinned for both the exact and probabilistic backends.
fn exclude_tags_merge_global_and_per_metric(mode: Mode) {
    let per_metric = PerMetricConfig {
        namespace: None,
        config: Inner {
            value_limit: 1,
            limit_exceeded_action: LimitExceededAction::DropTag,
            mode,
            internal_metrics: InternalMetricsConfig::default(),
            exclude_tags: vec!["tenant_id".to_string()],
        },
    };

    let config = Config {
        global: Inner {
            value_limit: 1,
            limit_exceeded_action: LimitExceededAction::DropTag,
            mode,
            internal_metrics: InternalMetricsConfig::default(),
            exclude_tags: vec!["kube_pod_name".to_string()],
        },
        per_metric_limits: HashMap::from([("metricA".to_string(), per_metric)]),
    };
    let mut transform = TagCardinalityLimit::new(config);

    // value_limit=1 means tag1's first value consumes the only slot.
    let event1 = make_metric_with_name(
        metric_tags!(
            "kube_pod_name" => "pod-a",
            "tenant_id" => "tenant-a",
            "tag1" => "val1"
        ),
        "metricA",
    );
    let event2 = make_metric_with_name(
        metric_tags!(
            "kube_pod_name" => "pod-b",
            "tenant_id" => "tenant-b",
            "tag1" => "val2"
        ),
        "metricA",
    );

    let tags1 = transform.transform_one(event1).unwrap();
    let tags1 = tags1.as_metric().tags().unwrap();
    assert_eq!("pod-a", tags1.get("kube_pod_name").unwrap());
    assert_eq!("tenant-a", tags1.get("tenant_id").unwrap());
    assert_eq!("val1", tags1.get("tag1").unwrap());

    let tags2 = transform.transform_one(event2).unwrap();
    let tags2 = tags2.as_metric().tags().unwrap();
    assert_eq!(
        "pod-b",
        tags2.get("kube_pod_name").unwrap(),
        "global exclusion must still apply to per-metric configs"
    );
    assert_eq!(
        "tenant-b",
        tags2.get("tenant_id").unwrap(),
        "per-metric exclusion must apply"
    );
    assert!(
        !tags2.contains_key("tag1"),
        "non-excluded tag1 should be dropped after exceeding per-metric value_limit"
    );
}

/// Pins the "never enter the cache" contract from the `exclude_tags` documentation by
/// asserting directly on `accepted_tags` rather than going through the public emission
/// surface. If this invariant breaks, excluded tags would silently start consuming memory
/// and producing the very internal events the docs promise they suppress.
#[test]
fn exclude_tags_never_populate_cache() {
    let config = make_transform_with_exclude_tags(
        2,
        LimitExceededAction::DropTag,
        Mode::Exact,
        vec!["kube_pod_name".to_string()],
    );
    let mut transform = TagCardinalityLimit::new(config);

    // Send well past `value_limit` distinct values for the excluded key.
    for i in 0..10 {
        let event = make_metric(metric_tags!(
            "kube_pod_name" => format!("pod-{i}").as_str(),
            "tag1" => "val1"
        ));
        transform.transform_one(event).unwrap();
    }

    let global_metric_entry = transform.accepted_tags.get(&None).expect(
        "non-excluded tag1 should still create an entry under the no-per-metric-key bucket",
    );
    assert!(
        global_metric_entry.contains_key("tag1"),
        "non-excluded tag must still be tracked"
    );
    assert!(
        !global_metric_entry.contains_key("kube_pod_name"),
        "excluded tag key must never enter the cache"
    );
}
