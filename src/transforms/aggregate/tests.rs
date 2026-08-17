use std::{collections::BTreeSet, sync::Arc, task::Poll, time::Duration};

use chrono::{DateTime, TimeZone, Utc};
use futures::{StreamExt, stream};
use indoc::indoc;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use vector_lib::config::{ComponentKey, LogNamespace};
use vrl::value::Kind;

use super::{
    Aggregate, AggregateConfig, AggregationMode, EventTimeConfig, MissingTimestamp,
    config::default_max_future_ms,
};
use crate::{
    config::{OutputId, TransformConfig, TransformContext},
    event::{
        BatchNotifier, BatchStatus, Event, EventStatus, Metric,
        metric::{MetricKind, MetricValue},
    },
    schema::Definition,
    test_util::components::assert_transform_compliance,
    transforms::test::create_topology,
};

#[test]
fn generate_config() {
    crate::test_util::test_generate_config::<AggregateConfig>();
}

#[test]
fn rejects_zero_interval_ms() {
    let result = Aggregate::new(&event_time_config(0, AggregationMode::Auto));

    let err = result.expect_err("zero interval_ms must not be accepted");
    assert!(
        err.to_string().contains("interval_ms"),
        "error should mention interval_ms",
    );
}

#[test]
fn validates_millisecond_fields_fit_i64() {
    let invalid_configs = [
        (
            event_time_config(u64::MAX, AggregationMode::Auto),
            "interval_ms",
        ),
        (
            event_time_config_with(1000, AggregationMode::Auto, |et| {
                et.max_future_ms = u64::MAX;
            }),
            "max_future_ms",
        ),
        (
            event_time_config_with(1000, AggregationMode::Auto, |et| {
                et.allowed_lateness_ms = u64::MAX;
            }),
            "allowed_lateness_ms",
        ),
    ];

    for (config, field) in invalid_configs {
        let error = Aggregate::new(&config).expect_err("u64::MAX must not be accepted");
        assert!(
            error.to_string().contains(field),
            "error should mention {field}, got: {error}"
        );
    }

    Aggregate::new(&event_time_config(i64::MAX as u64, AggregationMode::Auto))
        .expect("i64::MAX is the largest accepted interval");
    Aggregate::new(&event_time_config_with(1000, AggregationMode::Auto, |et| {
        et.max_future_ms = i64::MAX as u64;
        et.allowed_lateness_ms = i64::MAX as u64;
    }))
    .expect("i64::MAX is accepted for event-time millisecond fields");
}

fn make_metric(name: &'static str, kind: MetricKind, value: MetricValue) -> Event {
    let mut event = Event::Metric(Metric::new(name, kind, value))
        .with_source_id(Arc::new(ComponentKey::from("in")))
        .with_upstream_id(Arc::new(OutputId::from("transform")));
    event
        .metadata_mut()
        .set_schema_definition(&Arc::new(Definition::new_with_default_metadata(
            Kind::any_object(),
            [LogNamespace::Legacy],
        )));

    event.metadata_mut().set_source_type("unit_test_stream");

    event
}

fn make_metric_with_timestamp(
    name: &'static str,
    kind: MetricKind,
    value: MetricValue,
    timestamp: DateTime<Utc>,
) -> Event {
    let mut event = Event::Metric(Metric::new(name, kind, value).with_timestamp(Some(timestamp)))
        .with_source_id(Arc::new(ComponentKey::from("in")))
        .with_upstream_id(Arc::new(OutputId::from("transform")));
    event
        .metadata_mut()
        .set_schema_definition(&Arc::new(Definition::new_with_default_metadata(
            Kind::any_object(),
            [LogNamespace::Legacy],
        )));

    event.metadata_mut().set_source_type("unit_test_stream");

    event
}

/// Timestamp in the middle of the event-time bucket that is still open
/// for recording (`now` is before `bucket_end + allowed_lateness_ms`).
fn open_bucket_timestamp(interval_ms: u64) -> DateTime<Utc> {
    event_time_bucket_timestamp(interval_ms, 0)
}

/// Timestamp in the bucket `bucket_offset` intervals ahead of the current
/// open bucket (`0` = current, `1` = next, `-1` = previous, and so on).
fn event_time_bucket_timestamp(interval_ms: u64, bucket_offset: i64) -> DateTime<Utc> {
    let now_ms = Utc::now().timestamp_millis();
    let interval_i64 = i64::try_from(interval_ms).expect("test interval fits in i64");
    let current_key = now_ms.div_euclid(interval_i64).saturating_mul(interval_i64);
    let bucket_key = current_key.saturating_add(bucket_offset.saturating_mul(interval_i64));
    Utc.timestamp_millis_opt(bucket_key + interval_i64 / 2)
        .single()
        .expect("bucket midpoint is a valid timestamp")
}

fn system_time_config(mode: AggregationMode) -> AggregateConfig {
    AggregateConfig {
        interval_ms: 1000,
        mode,
        event_time: None,
    }
}

fn event_time_config(interval_ms: u64, mode: AggregationMode) -> AggregateConfig {
    AggregateConfig {
        interval_ms,
        mode,
        event_time: Some(EventTimeConfig::default()),
    }
}

fn event_time_config_with(
    interval_ms: u64,
    mode: AggregationMode,
    f: impl FnOnce(&mut EventTimeConfig),
) -> AggregateConfig {
    let mut event_time = EventTimeConfig::default();
    f(&mut event_time);
    AggregateConfig {
        interval_ms,
        mode,
        event_time: Some(event_time),
    }
}

#[test]
fn incremental_auto() {
    let mut agg = Aggregate::new(&system_time_config(AggregationMode::Auto)).unwrap();

    let counter_a_1 = make_metric(
        "counter_a",
        MetricKind::Incremental,
        MetricValue::Counter { value: 42.0 },
    );
    let counter_a_2 = make_metric(
        "counter_a",
        MetricKind::Incremental,
        MetricValue::Counter { value: 43.0 },
    );
    let counter_a_summed = make_metric(
        "counter_a",
        MetricKind::Incremental,
        MetricValue::Counter { value: 85.0 },
    );

    // Single item, just stored regardless of kind
    agg.record(counter_a_1.clone());
    let mut out = vec![];
    // We should flush 1 item counter_a_1
    agg.flush_into(&mut out);
    assert_eq!(1, out.len());
    assert_eq!(&counter_a_1, &out[0]);

    // A subsequent flush doesn't send out anything
    out.clear();
    agg.flush_into(&mut out);
    assert_eq!(0, out.len());

    // One more just to make sure that we don't re-see from the other buffer
    out.clear();
    agg.flush_into(&mut out);
    assert_eq!(0, out.len());

    // Two increments with the same series, should sum into 1
    agg.record(counter_a_1.clone());
    agg.record(counter_a_2);
    out.clear();
    agg.flush_into(&mut out);
    assert_eq!(1, out.len());
    assert_eq!(&counter_a_summed, &out[0]);

    let counter_b_1 = make_metric(
        "counter_b",
        MetricKind::Incremental,
        MetricValue::Counter { value: 44.0 },
    );
    // Two increments with the different series, should get each back as-is
    agg.record(counter_a_1.clone());
    agg.record(counter_b_1.clone());
    out.clear();
    agg.flush_into(&mut out);
    assert_eq!(2, out.len());
    // B/c we don't know the order they'll come back
    for event in out {
        match event.as_metric().series().name.name.as_str() {
            "counter_a" => assert_eq!(counter_a_1, event),
            "counter_b" => assert_eq!(counter_b_1, event),
            _ => panic!("Unexpected metric name in aggregate output"),
        }
    }
}

#[test]
fn passes_through_ignored_kind() {
    // Sum mode aggregates incremental, passes through absolute without collapsing.
    let mut agg = Aggregate::new(&system_time_config(AggregationMode::Sum)).unwrap();

    let counter_1 = make_metric(
        "counter_a",
        MetricKind::Incremental,
        MetricValue::Counter { value: 10.0 },
    );
    let counter_2 = make_metric(
        "counter_a",
        MetricKind::Incremental,
        MetricValue::Counter { value: 5.0 },
    );
    let counter_summed = make_metric(
        "counter_a",
        MetricKind::Incremental,
        MetricValue::Counter { value: 15.0 },
    );
    let gauge_1 = make_metric(
        "gauge_a",
        MetricKind::Absolute,
        MetricValue::Gauge { value: 42.0 },
    );
    let gauge_2 = make_metric(
        "gauge_a",
        MetricKind::Absolute,
        MetricValue::Gauge { value: 99.0 },
    );

    // Absolute metrics pass through immediately (not held until flush).
    assert_eq!(agg.record(gauge_1.clone()), Some(gauge_1));
    assert_eq!(agg.record(gauge_2.clone()), Some(gauge_2));

    // Each is returned individually — no collapsing to latest.
    assert_eq!(agg.record(counter_1), None);
    assert_eq!(agg.record(counter_2), None);

    let mut out = vec![];
    agg.flush_into(&mut out);
    // Only the summed incremental counter appears at flush; the gauges already passed through.
    assert_eq!(1, out.len());
    assert_eq!(&counter_summed, &out[0]);
}

#[test]
fn absolute_auto() {
    let mut agg = Aggregate::new(&system_time_config(AggregationMode::Auto)).unwrap();

    let gauge_a_1 = make_metric(
        "gauge_a",
        MetricKind::Absolute,
        MetricValue::Gauge { value: 42.0 },
    );
    let gauge_a_2 = make_metric(
        "gauge_a",
        MetricKind::Absolute,
        MetricValue::Gauge { value: 43.0 },
    );

    // Single item, just stored regardless of kind
    agg.record(gauge_a_1.clone());
    let mut out = vec![];
    // We should flush 1 item gauge_a_1
    agg.flush_into(&mut out);
    assert_eq!(1, out.len());
    assert_eq!(&gauge_a_1, &out[0]);

    // A subsequent flush doesn't send out anything
    out.clear();
    agg.flush_into(&mut out);
    assert_eq!(0, out.len());

    // One more just to make sure that we don't re-see from the other buffer
    out.clear();
    agg.flush_into(&mut out);
    assert_eq!(0, out.len());

    // Two absolutes with the same series, should get the 2nd (last) back.
    agg.record(gauge_a_1.clone());
    agg.record(gauge_a_2.clone());
    out.clear();
    agg.flush_into(&mut out);
    assert_eq!(1, out.len());
    assert_eq!(&gauge_a_2, &out[0]);

    let gauge_b_1 = make_metric(
        "gauge_b",
        MetricKind::Absolute,
        MetricValue::Gauge { value: 44.0 },
    );
    // Two increments with the different series, should get each back as-is
    agg.record(gauge_a_1.clone());
    agg.record(gauge_b_1.clone());
    out.clear();
    agg.flush_into(&mut out);
    assert_eq!(2, out.len());
    // B/c we don't know the order they'll come back
    for event in out {
        match event.as_metric().series().name.name.as_str() {
            "gauge_a" => assert_eq!(gauge_a_1, event),
            "gauge_b" => assert_eq!(gauge_b_1, event),
            _ => panic!("Unexpected metric name in aggregate output"),
        }
    }
}

#[test]
fn count_agg() {
    let mut agg = Aggregate::new(&system_time_config(AggregationMode::Count)).unwrap();

    let gauge_a_1 = make_metric(
        "gauge_a",
        MetricKind::Absolute,
        MetricValue::Gauge { value: 42.0 },
    );
    let gauge_a_2 = make_metric(
        "gauge_a",
        MetricKind::Absolute,
        MetricValue::Gauge { value: 43.0 },
    );
    let result_count = make_metric(
        "gauge_a",
        MetricKind::Absolute,
        MetricValue::Counter { value: 1.0 },
    );
    let result_count_2 = make_metric(
        "gauge_a",
        MetricKind::Absolute,
        MetricValue::Counter { value: 2.0 },
    );

    // Single item, counter should be 1
    agg.record(gauge_a_1.clone());
    let mut out = vec![];
    // We should flush 1 item gauge_a_1
    agg.flush_into(&mut out);
    assert_eq!(1, out.len());
    assert_eq!(&result_count, &out[0]);

    // A subsequent flush doesn't send out anything
    out.clear();
    agg.flush_into(&mut out);
    assert_eq!(0, out.len());

    // One more just to make sure that we don't re-see from the other buffer
    out.clear();
    agg.flush_into(&mut out);
    assert_eq!(0, out.len());

    // Two absolutes with the same series, counter should be 2
    agg.record(gauge_a_1.clone());
    agg.record(gauge_a_2.clone());
    out.clear();
    agg.flush_into(&mut out);
    assert_eq!(1, out.len());
    assert_eq!(&result_count_2, &out[0]);
}

#[test]
fn absolute_max() {
    let mut agg = Aggregate::new(&system_time_config(AggregationMode::Max)).unwrap();

    let gauge_a_1 = make_metric(
        "gauge_a",
        MetricKind::Absolute,
        MetricValue::Gauge { value: 112.0 },
    );
    let gauge_a_2 = make_metric(
        "gauge_a",
        MetricKind::Absolute,
        MetricValue::Gauge { value: 89.0 },
    );

    // Single item, it should be returned as is
    agg.record(gauge_a_2.clone());
    let mut out = vec![];
    // We should flush 1 item gauge_a_2
    agg.flush_into(&mut out);
    assert_eq!(1, out.len());
    assert_eq!(&gauge_a_2, &out[0]);

    // A subsequent flush doesn't send out anything
    out.clear();
    agg.flush_into(&mut out);
    assert_eq!(0, out.len());

    // One more just to make sure that we don't re-see from the other buffer
    out.clear();
    agg.flush_into(&mut out);
    assert_eq!(0, out.len());

    // Two absolutes, result should be higher of the 2
    agg.record(gauge_a_1.clone());
    agg.record(gauge_a_2.clone());
    out.clear();
    agg.flush_into(&mut out);
    assert_eq!(1, out.len());
    assert_eq!(&gauge_a_1, &out[0]);
}

#[test]
fn absolute_min() {
    let mut agg = Aggregate::new(&system_time_config(AggregationMode::Min)).unwrap();

    let gauge_a_1 = make_metric(
        "gauge_a",
        MetricKind::Absolute,
        MetricValue::Gauge { value: 32.0 },
    );
    let gauge_a_2 = make_metric(
        "gauge_a",
        MetricKind::Absolute,
        MetricValue::Gauge { value: 89.0 },
    );

    // Single item, it should be returned as is
    agg.record(gauge_a_2.clone());
    let mut out = vec![];
    // We should flush 1 item gauge_a_2
    agg.flush_into(&mut out);
    assert_eq!(1, out.len());
    assert_eq!(&gauge_a_2, &out[0]);

    // A subsequent flush doesn't send out anything
    out.clear();
    agg.flush_into(&mut out);
    assert_eq!(0, out.len());

    // One more just to make sure that we don't re-see from the other buffer
    out.clear();
    agg.flush_into(&mut out);
    assert_eq!(0, out.len());

    // Two absolutes, result should be lower of the 2
    agg.record(gauge_a_1.clone());
    agg.record(gauge_a_2.clone());
    out.clear();
    agg.flush_into(&mut out);
    assert_eq!(1, out.len());
    assert_eq!(&gauge_a_1, &out[0]);
}

#[test]
fn absolute_diff() {
    let mut agg = Aggregate::new(&system_time_config(AggregationMode::Diff)).unwrap();

    let gauge_a_1 = make_metric(
        "gauge_a",
        MetricKind::Absolute,
        MetricValue::Gauge { value: 32.0 },
    );
    let gauge_a_2 = make_metric(
        "gauge_a",
        MetricKind::Absolute,
        MetricValue::Gauge { value: 82.0 },
    );
    let result = make_metric(
        "gauge_a",
        MetricKind::Absolute,
        MetricValue::Gauge { value: 50.0 },
    );

    // Single item, it should be returned as is
    agg.record(gauge_a_2.clone());
    let mut out = vec![];
    // We should flush 1 item gauge_a_2
    agg.flush_into(&mut out);
    assert_eq!(1, out.len());
    assert_eq!(&gauge_a_2, &out[0]);

    // A subsequent flush doesn't send out anything
    out.clear();
    agg.flush_into(&mut out);
    assert_eq!(0, out.len());

    // One more just to make sure that we don't re-see from the other buffer
    out.clear();
    agg.flush_into(&mut out);
    assert_eq!(0, out.len());

    // Two absolutes in 2 separate flushes, result should be diff between the 2
    agg.record(gauge_a_1.clone());
    out.clear();
    agg.flush_into(&mut out);
    assert_eq!(1, out.len());
    assert_eq!(&gauge_a_1, &out[0]);

    agg.record(gauge_a_2.clone());
    out.clear();
    agg.flush_into(&mut out);
    assert_eq!(1, out.len());
    assert_eq!(&result, &out[0]);
}

#[test]
fn absolute_diff_conflicting_type() {
    let mut agg = Aggregate::new(&system_time_config(AggregationMode::Diff)).unwrap();

    let gauge_a_1 = make_metric(
        "gauge_a",
        MetricKind::Absolute,
        MetricValue::Gauge { value: 32.0 },
    );
    let gauge_a_2 = make_metric(
        "gauge_a",
        MetricKind::Absolute,
        MetricValue::Counter { value: 1.0 },
    );

    let mut out = vec![];
    // Two absolutes in 2 separate flushes, result should be second one due to different types
    agg.record(gauge_a_1.clone());
    out.clear();
    agg.flush_into(&mut out);
    assert_eq!(1, out.len());
    assert_eq!(&gauge_a_1, &out[0]);

    agg.record(gauge_a_2.clone());
    out.clear();
    agg.flush_into(&mut out);
    assert_eq!(1, out.len());
    // Due to incompatible results, the new value just overwrites the old one
    assert_eq!(&gauge_a_2, &out[0]);
}

#[test]
fn absolute_mean() {
    let mut agg = Aggregate::new(&system_time_config(AggregationMode::Mean)).unwrap();

    let gauge_a_1 = make_metric(
        "gauge_a",
        MetricKind::Absolute,
        MetricValue::Gauge { value: 32.0 },
    );
    let gauge_a_2 = make_metric(
        "gauge_a",
        MetricKind::Absolute,
        MetricValue::Gauge { value: 82.0 },
    );
    let gauge_a_3 = make_metric(
        "gauge_a",
        MetricKind::Absolute,
        MetricValue::Gauge { value: 51.0 },
    );
    let mean_result = make_metric(
        "gauge_a",
        MetricKind::Absolute,
        MetricValue::Gauge { value: 55.0 },
    );

    // Single item, it should be returned as is
    agg.record(gauge_a_2.clone());
    let mut out = vec![];
    // We should flush 1 item gauge_a_2
    agg.flush_into(&mut out);
    assert_eq!(1, out.len());
    assert_eq!(&gauge_a_2, &out[0]);

    // A subsequent flush doesn't send out anything
    out.clear();
    agg.flush_into(&mut out);
    assert_eq!(0, out.len());

    // One more just to make sure that we don't re-see from the other buffer
    out.clear();
    agg.flush_into(&mut out);
    assert_eq!(0, out.len());

    // Three absolutes, result should be mean
    agg.record(gauge_a_1.clone());
    agg.record(gauge_a_2.clone());
    agg.record(gauge_a_3.clone());
    out.clear();
    agg.flush_into(&mut out);
    assert_eq!(1, out.len());
    assert_eq!(&mean_result, &out[0]);
}

#[test]
fn absolute_stdev() {
    let mut agg = Aggregate::new(&system_time_config(AggregationMode::Stdev)).unwrap();

    let gauges = vec![
        make_metric(
            "gauge_a",
            MetricKind::Absolute,
            MetricValue::Gauge { value: 25.0 },
        ),
        make_metric(
            "gauge_a",
            MetricKind::Absolute,
            MetricValue::Gauge { value: 30.0 },
        ),
        make_metric(
            "gauge_a",
            MetricKind::Absolute,
            MetricValue::Gauge { value: 35.0 },
        ),
        make_metric(
            "gauge_a",
            MetricKind::Absolute,
            MetricValue::Gauge { value: 40.0 },
        ),
        make_metric(
            "gauge_a",
            MetricKind::Absolute,
            MetricValue::Gauge { value: 45.0 },
        ),
        make_metric(
            "gauge_a",
            MetricKind::Absolute,
            MetricValue::Gauge { value: 50.0 },
        ),
        make_metric(
            "gauge_a",
            MetricKind::Absolute,
            MetricValue::Gauge { value: 55.0 },
        ),
    ];
    let stdev_result = make_metric(
        "gauge_a",
        MetricKind::Absolute,
        MetricValue::Gauge { value: 10.0 },
    );

    for gauge in gauges {
        agg.record(gauge);
    }
    let mut out = vec![];
    agg.flush_into(&mut out);
    assert_eq!(1, out.len());
    assert_eq!(&stdev_result, &out[0]);
}

#[test]
fn conflicting_value_type() {
    let mut agg = Aggregate::new(&system_time_config(AggregationMode::Auto)).unwrap();

    let counter = make_metric(
        "the-thing",
        MetricKind::Incremental,
        MetricValue::Counter { value: 42.0 },
    );
    let mut values = BTreeSet::<String>::new();
    values.insert("a".into());
    values.insert("b".into());
    let set = make_metric(
        "the-thing",
        MetricKind::Incremental,
        MetricValue::Set { values },
    );
    let summed = make_metric(
        "the-thing",
        MetricKind::Incremental,
        MetricValue::Counter { value: 84.0 },
    );

    // when types conflict the new values replaces whatever is there

    // Start with an counter
    agg.record(counter.clone());
    // Another will "add" to it
    agg.record(counter.clone());
    // Then an set will replace it due to a failed update
    agg.record(set.clone());
    // Then a set union would be a noop
    agg.record(set.clone());
    let mut out = vec![];
    // We should flush 1 item counter
    agg.flush_into(&mut out);
    assert_eq!(1, out.len());
    assert_eq!(&set, &out[0]);

    // Start out with an set
    agg.record(set.clone());
    // Union with itself, a noop
    agg.record(set);
    // Send an counter with the same name, will replace due to a failed update
    agg.record(counter.clone());
    // Send another counter will "add"
    agg.record(counter);
    let mut out = vec![];
    // We should flush 1 item counter
    agg.flush_into(&mut out);
    assert_eq!(1, out.len());
    assert_eq!(&summed, &out[0]);
}

#[test]
fn conflicting_kinds() {
    let mut agg = Aggregate::new(&system_time_config(AggregationMode::Auto)).unwrap();

    let incremental = make_metric(
        "the-thing",
        MetricKind::Incremental,
        MetricValue::Counter { value: 42.0 },
    );
    let absolute = make_metric(
        "the-thing",
        MetricKind::Absolute,
        MetricValue::Counter { value: 43.0 },
    );
    let summed = make_metric(
        "the-thing",
        MetricKind::Incremental,
        MetricValue::Counter { value: 84.0 },
    );

    // when types conflict the new values replaces whatever is there

    // Start with an incremental
    agg.record(incremental.clone());
    // Another will "add" to it
    agg.record(incremental.clone());
    // Then an absolute will replace it with a failed update
    agg.record(absolute.clone());
    // Then another absolute will replace it normally
    agg.record(absolute.clone());
    let mut out = vec![];
    // We should flush 1 item incremental
    agg.flush_into(&mut out);
    assert_eq!(1, out.len());
    assert_eq!(&absolute, &out[0]);

    // Start out with an absolute
    agg.record(absolute.clone());
    // Replace it normally
    agg.record(absolute);
    // Send an incremental with the same name, will replace due to a failed update
    agg.record(incremental.clone());
    // Send another incremental will "add"
    agg.record(incremental);
    let mut out = vec![];
    // We should flush 1 item incremental
    agg.flush_into(&mut out);
    assert_eq!(1, out.len());
    assert_eq!(&summed, &out[0]);
}

#[tokio::test]
async fn transform_shutdown() {
    let agg = serde_yaml::from_str::<AggregateConfig>(indoc! {"
        interval_ms: 999999
    "})
    .unwrap()
    .build(&TransformContext::default())
    .await
    .unwrap();

    let agg = agg.into_task();

    let counter_a_1 = make_metric(
        "counter_a",
        MetricKind::Incremental,
        MetricValue::Counter { value: 42.0 },
    );
    let counter_a_2 = make_metric(
        "counter_a",
        MetricKind::Incremental,
        MetricValue::Counter { value: 43.0 },
    );
    let counter_a_summed = make_metric(
        "counter_a",
        MetricKind::Incremental,
        MetricValue::Counter { value: 85.0 },
    );
    let gauge_a_1 = make_metric(
        "gauge_a",
        MetricKind::Absolute,
        MetricValue::Gauge { value: 42.0 },
    );
    let gauge_a_2 = make_metric(
        "gauge_a",
        MetricKind::Absolute,
        MetricValue::Gauge { value: 43.0 },
    );
    let inputs = vec![counter_a_1, counter_a_2, gauge_a_1, gauge_a_2.clone()];

    // Queue up some events to be consumed & recorded
    let in_stream = Box::pin(stream::iter(inputs));
    // Kick off the transform process which should consume & record them
    let mut out_stream = agg.transform_events(in_stream);

    // B/c the input stream has ended we will have gone through the `input_rx.next() => None`
    // part of the loop and do the shutting down final flush immediately. We'll already be able
    // to read our expected bits on the output.
    let mut count = 0_u8;
    while let Some(event) = out_stream.next().await {
        count += 1;
        match event.as_metric().series().name.name.as_str() {
            "counter_a" => assert_eq!(counter_a_summed, event),
            "gauge_a" => assert_eq!(gauge_a_2, event),
            _ => panic!("Unexpected metric name in aggregate output"),
        };
    }
    // There were only 2
    assert_eq!(2, count);
}

#[tokio::test]
async fn transform_interval() {
    let transform_config = serde_yaml::from_str::<AggregateConfig>("{}").unwrap();

    let counter_a_1 = make_metric(
        "counter_a",
        MetricKind::Incremental,
        MetricValue::Counter { value: 42.0 },
    );
    let counter_a_2 = make_metric(
        "counter_a",
        MetricKind::Incremental,
        MetricValue::Counter { value: 43.0 },
    );
    let counter_a_summed = make_metric(
        "counter_a",
        MetricKind::Incremental,
        MetricValue::Counter { value: 85.0 },
    );
    let gauge_a_1 = make_metric(
        "gauge_a",
        MetricKind::Absolute,
        MetricValue::Gauge { value: 42.0 },
    );
    let gauge_a_2 = make_metric(
        "gauge_a",
        MetricKind::Absolute,
        MetricValue::Gauge { value: 43.0 },
    );

    assert_transform_compliance(async {
        let (tx, rx) = mpsc::channel(10);
        let (topology, out) = create_topology(ReceiverStream::new(rx), transform_config).await;
        let mut out = ReceiverStream::new(out);

        tokio::time::pause();

        // tokio interval is always immediately ready, so we poll once to make sure
        // we trip it/set the interval in the future
        assert_eq!(Poll::Pending, futures::poll!(out.next()));

        // Now send our events
        tx.send(counter_a_1).await.unwrap();
        tx.send(counter_a_2).await.unwrap();
        tx.send(gauge_a_1).await.unwrap();
        tx.send(gauge_a_2.clone()).await.unwrap();
        // We won't have flushed yet b/c the interval hasn't elapsed, so no outputs
        assert_eq!(Poll::Pending, futures::poll!(out.next()));
        // Now fast forward time enough that our flush should trigger.
        tokio::time::advance(Duration::from_secs(11)).await;
        // We should have had an interval fire now and our output aggregate events should be
        // available.
        let mut count = 0_u8;
        while count < 2 {
            match out.next().await {
                Some(event) => {
                    match event.as_metric().series().name.name.as_str() {
                        "counter_a" => assert_eq!(counter_a_summed, event),
                        "gauge_a" => assert_eq!(gauge_a_2, event),
                        _ => panic!("Unexpected metric name in aggregate output"),
                    };
                    count += 1;
                }
                _ => {
                    panic!("Unexpectedly received None in output stream");
                }
            }
        }
        // We should be back to pending, having nothing waiting for us
        assert_eq!(Poll::Pending, futures::poll!(out.next()));

        drop(tx);
        topology.stop().await;
        assert_eq!(out.next().await, None);
    })
    .await;
}

/// Rust truncating `/` rounds toward zero, so `-1 / 10000 == 0` and the
/// bucket anchor would wrongly be `0`. Euclidean alignment places
/// `-1ms` in `[-interval_ms, 0)` anchored at `-interval_ms`.
#[test]
fn event_time_pre_epoch_buckets_use_floor_division() {
    let agg = Aggregate::new(&event_time_config(10_000_u64, AggregationMode::Auto)).unwrap();

    let ts = Utc
        .timestamp_millis_opt(-1)
        .latest()
        .expect("valid millis near epoch");

    assert_eq!(
        agg.bucket_key(ts),
        -10_000,
        "-1 ms must bucket to [-10000, 0), not [0, 10000)"
    );
}

#[test]
fn event_time_different_buckets() {
    let mut agg = Aggregate::new(&event_time_config_with(
        10000_u64,
        AggregationMode::Auto,
        |et| {
            et.max_future_ms = 600_000;
        },
    ))
    .unwrap();

    let base_time = open_bucket_timestamp(10_000);

    // Events in first bucket (11:00:20 - 11:00:30)
    let event1_bucket1 = make_metric_with_timestamp(
        "counter_a",
        MetricKind::Incremental,
        MetricValue::Counter { value: 10.0 },
        base_time,
    );
    let event2_bucket1 = make_metric_with_timestamp(
        "counter_a",
        MetricKind::Incremental,
        MetricValue::Counter { value: 20.0 },
        base_time + chrono::Duration::milliseconds(500),
    );

    let event1_bucket2 = make_metric_with_timestamp(
        "counter_a",
        MetricKind::Incremental,
        MetricValue::Counter { value: 30.0 },
        event_time_bucket_timestamp(10_000, 1),
    );

    // Record events from first bucket
    agg.record(event1_bucket1);
    agg.record(event2_bucket1);
    let mut out = vec![];
    agg.flush_final(&mut out);
    // Should flush first bucket (summed: 10 + 20 = 30)
    assert_eq!(1, out.len());
    let metric = out[0].as_metric();
    if let MetricValue::Counter { value } = metric.value() {
        assert_eq!(*value, 30.0);
    } else {
        panic!("Expected Counter value");
    }

    // Record event from second bucket
    agg.record(event1_bucket2);
    out.clear();
    agg.flush_final(&mut out);
    // Should flush second bucket (30.0)
    assert_eq!(1, out.len());
    let metric = out[0].as_metric();
    if let MetricValue::Counter { value } = metric.value() {
        assert_eq!(*value, 30.0);
    } else {
        panic!("Expected Counter value");
    }
}

#[test]
fn event_time_absolute_latest() {
    let mut agg = Aggregate::new(&event_time_config(10000_u64, AggregationMode::Auto)).unwrap();

    let base_time = open_bucket_timestamp(10_000);

    // Multiple absolute metrics in same bucket
    let gauge1 = make_metric_with_timestamp(
        "gauge_a",
        MetricKind::Absolute,
        MetricValue::Gauge { value: 42.0 },
        base_time,
    );
    let gauge2 = make_metric_with_timestamp(
        "gauge_a",
        MetricKind::Absolute,
        MetricValue::Gauge { value: 43.0 },
        base_time + chrono::Duration::milliseconds(500),
    );

    // Record out of timestamp order: latest means latest event timestamp, not arrival order.
    agg.record(gauge2);
    agg.record(gauge1);
    let mut out = vec![];
    agg.flush_final(&mut out);
    // Should get the latest value (43.0)
    assert_eq!(1, out.len());
    let metric = out[0].as_metric();
    if let MetricValue::Gauge { value } = metric.value() {
        assert_eq!(*value, 43.0);
    } else {
        panic!("Expected Gauge value");
    }
}

#[test]
fn event_time_diff_uses_previous_bucket() {
    let mut agg = Aggregate::new(&event_time_config_with(
        10000_u64,
        AggregationMode::Diff,
        |et| {
            et.max_future_ms = 600_000;
        },
    ))
    .unwrap();

    let ts1 = event_time_bucket_timestamp(10_000, 0);
    let ts2 = event_time_bucket_timestamp(10_000, 1);

    let g1 = make_metric_with_timestamp(
        "diff_gauge",
        MetricKind::Absolute,
        MetricValue::Gauge { value: 10.0 },
        ts1,
    );
    let g2 = make_metric_with_timestamp(
        "diff_gauge",
        MetricKind::Absolute,
        MetricValue::Gauge { value: 25.0 },
        ts2,
    );

    agg.record(g1);
    agg.record(g2);

    let mut out = vec![];
    agg.flush_final(&mut out);

    assert_eq!(2, out.len());

    let mut values: Vec<f64> = out
        .iter()
        .map(|event| {
            let metric = event.as_metric();
            if let MetricValue::Gauge { value } = metric.value() {
                *value
            } else {
                panic!("Expected Gauge metric value");
            }
        })
        .collect();

    values.sort_by(|a, b| a.partial_cmp(b).unwrap());
    assert!((values[0] - 10.0).abs() < 1e-9);
    assert!((values[1] - 15.0).abs() < 1e-9);
}

/// With `allowed_lateness_ms = 0`, events must be rejected once the
/// bucket window has ended on the wall clock, even if the periodic flush
/// tick has not yet closed the bucket.
#[test]
fn event_time_rejects_late_events_before_flush_tick() {
    let interval_ms = 10_000_u64;

    let agg = Aggregate::new(&event_time_config(interval_ms, AggregationMode::Auto)).unwrap();

    let now_ms = Utc::now().timestamp_millis();
    let interval_i64 = interval_ms as i64;
    let ended_bucket_key = (now_ms / interval_i64) * interval_i64 - interval_i64 * 2;
    assert!(
        agg.is_past_bucket_cutoff(ended_bucket_key, now_ms),
        "wall clock must be past a bucket two intervals old"
    );

    let mut agg = agg;
    let ts = Utc
        .timestamp_millis_opt(ended_bucket_key + interval_i64 / 2)
        .latest()
        .unwrap();
    assert!(
        agg.record(make_metric_with_timestamp(
            "late_before_flush",
            MetricKind::Absolute,
            MetricValue::Gauge { value: 1.0 },
            ts,
        ))
        .is_none(),
        "events for an ended window must be dropped before flush"
    );
    assert!(agg.event_time_buckets.is_empty());
}

#[test]
fn event_time_closed_buckets_are_rejected_regardless_of_grace() {
    // Once a bucket has been emitted, a late event for that bucket (or any
    // earlier one) must be rejected even if `allowed_lateness_ms` would
    // still permit recording into an unflushed bucket.
    let interval_ms = 10_000_u64;

    let mut agg = Aggregate::new(&event_time_config_with(
        interval_ms,
        AggregationMode::Auto,
        |et| {
            et.allowed_lateness_ms = 10_000;
            et.max_future_ms = 600_000;
        },
    ))
    .unwrap();

    let interval_i64 = interval_ms as i64;
    let bucket0 = open_bucket_timestamp(interval_ms);
    let bucket0_key = bucket0.timestamp_millis().div_euclid(interval_i64) * interval_i64;
    let bucket_end = bucket0_key + interval_i64;

    agg.record(make_metric_with_timestamp(
        "gauge_closed_bucket_rejection",
        MetricKind::Absolute,
        MetricValue::Gauge { value: 1.0 },
        bucket0,
    ));
    let mut out = vec![];
    agg.flush_final(&mut out);
    assert_eq!(1, out.len());
    assert_eq!(agg.watermark, Some(bucket_end));
    out.clear();

    agg.record(make_metric_with_timestamp(
        "gauge_closed_bucket_rejection",
        MetricKind::Absolute,
        MetricValue::Gauge { value: 99.0 },
        bucket0 + chrono::Duration::milliseconds(500),
    ));
    agg.record(make_metric_with_timestamp(
        "gauge_closed_bucket_rejection",
        MetricKind::Absolute,
        MetricValue::Gauge { value: 99.0 },
        event_time_bucket_timestamp(interval_ms, -1),
    ));
    agg.flush_final(&mut out);
    assert!(
        out.is_empty(),
        "events for already-closed buckets must not produce a duplicate aggregate"
    );

    agg.record(make_metric_with_timestamp(
        "gauge_closed_bucket_rejection",
        MetricKind::Absolute,
        MetricValue::Gauge { value: 7.0 },
        event_time_bucket_timestamp(interval_ms, 1),
    ));
    agg.flush_final(&mut out);
    assert_eq!(1, out.len(), "next open bucket must still flush");
    if let MetricValue::Gauge { value } = out[0].as_metric().value() {
        assert_eq!(*value, 7.0);
    } else {
        panic!("Expected Gauge metric value");
    }
}

#[test]
fn event_time_previous_bucket_retention_is_mode_specific_and_bounded() {
    let interval_ms = 10_000_u64;

    for mode in [AggregationMode::Auto, AggregationMode::Diff] {
        let mut agg = Aggregate::new(&event_time_config_with(interval_ms, mode, |et| {
            et.max_future_ms = 600_000;
        }))
        .unwrap();

        for offset in 0..50_i64 {
            agg.record(make_metric_with_timestamp(
                "retention_probe",
                if mode == AggregationMode::Diff {
                    MetricKind::Absolute
                } else {
                    MetricKind::Incremental
                },
                if mode == AggregationMode::Diff {
                    MetricValue::Gauge { value: 1.0 }
                } else {
                    MetricValue::Counter { value: 1.0 }
                },
                event_time_bucket_timestamp(interval_ms, offset),
            ));
            let mut out = vec![];
            agg.flush_final(&mut out);
            assert_eq!(out.len(), 1);
        }

        let retained = agg.event_time_prev_buckets.len();
        if mode == AggregationMode::Diff {
            assert!(
                retained <= 2,
                "Diff retention must stay bounded, got {retained}"
            );
        } else {
            assert_eq!(retained, 0, "non-Diff modes must retain no prior buckets");
        }
    }
}

/// `event_time.missing_timestamp` parses the snake_case literals shown in docs.
#[test]
fn event_time_block_parses_documented_literals() {
    use crate::config::GenerateConfig;

    let generated = AggregateConfig::generate_config();
    assert!(
        generated.get("event_time").is_none(),
        "system-time default must not emit an event_time block"
    );
    assert_eq!(
        EventTimeConfig::default().max_future_ms,
        default_max_future_ms()
    );

    let cfg: AggregateConfig = toml::from_str(
        r#"
        [event_time]
        missing_timestamp = "use_system_time"
        allowed_lateness_ms = 5000
        max_future_ms = 60000
    "#,
    )
    .unwrap();
    let event_time = cfg.event_time.expect("event_time block present");
    assert_eq!(
        event_time.missing_timestamp,
        MissingTimestamp::UseSystemTime
    );
    assert_eq!(event_time.allowed_lateness_ms, 5000);
    assert_eq!(event_time.max_future_ms, 60000);

    let cfg: AggregateConfig = toml::from_str(
        r#"
        [event_time]
        missing_timestamp = "drop"
    "#,
    )
    .unwrap();
    assert_eq!(
        cfg.event_time.unwrap().missing_timestamp,
        MissingTimestamp::Drop
    );
}

/// When the input stream closes, every still-open event-time bucket must
/// be drained so in-flight metrics are not silently dropped on shutdown
/// or topology reload (matching system-time semantics, where
/// `flush_system_time` always empties the entire map).
#[tokio::test]
async fn event_time_drains_open_buckets_on_shutdown() {
    // Long interval and large grace so the bucket is *not* eligible for the
    // wall-clock flush during the test -- the only way it gets emitted is
    // via the final-flush shutdown path.
    let agg = toml::from_str::<AggregateConfig>(
        r#"
interval_ms = 600000
[event_time]
allowed_lateness_ms = 600000
"#,
    )
    .unwrap()
    .build(&TransformContext::default())
    .await
    .unwrap()
    .into_task();

    let event = make_metric_with_timestamp(
        "shutdown_drain_probe",
        MetricKind::Incremental,
        MetricValue::Counter { value: 41.0 },
        Utc::now(),
    );

    let in_stream = Box::pin(stream::iter(vec![event]));
    let mut out_stream = agg.transform_events(in_stream);

    let mut count = 0_u8;
    while let Some(ev) = out_stream.next().await {
        count += 1;
        assert_eq!(
            ev.as_metric().series().name.name.as_str(),
            "shutdown_drain_probe"
        );
        if let MetricValue::Counter { value } = ev.as_metric().value() {
            assert_eq!(*value, 41.0);
        } else {
            panic!("Expected Counter metric value from drained bucket");
        }
    }
    assert_eq!(
        count, 1,
        "open event-time bucket must be drained when the input stream closes"
    );
}

#[test]
fn event_time_future_timestamp_rejected() {
    let mut agg = Aggregate::new(&event_time_config_with(
        10000_u64,
        AggregationMode::Auto,
        |et| {
            et.max_future_ms = 5000;
        },
    ))
    .unwrap();

    // Timestamp 60 seconds in the future — well beyond max_future_ms.
    let far_future = Utc::now() + chrono::Duration::seconds(60);
    let event = make_metric_with_timestamp(
        "counter_future",
        MetricKind::Incremental,
        MetricValue::Counter { value: 99.0 },
        far_future,
    );

    agg.record(event);
    let mut out = vec![];
    agg.flush_into(&mut out);
    assert_eq!(0, out.len(), "Far-future event must be dropped");
}

/// `Aggregate::new` accepts `max_future_ms: i64::MAX as u64` -- the
/// largest non-wrapping i64 cast -- but a naive future-skew check that
/// computes `Utc::now() + Duration::milliseconds(i64::MAX)` overflows
/// `NaiveDateTime`'s representable range and panics in `<DateTime as
/// Add<Duration>>::add` as soon as the first event is recorded. The
/// check must instead compare drift in millis with saturating math so
/// the boundary value is safe at record time.
#[test]
fn event_time_record_with_max_future_ms_at_i64_max_does_not_panic() {
    let mut agg = Aggregate::new(&event_time_config_with(1000, AggregationMode::Auto, |et| {
        et.max_future_ms = i64::MAX as u64;
    }))
    .unwrap();

    let event = make_metric_with_timestamp(
        "counter_now",
        MetricKind::Incremental,
        MetricValue::Counter { value: 1.0 },
        Utc::now(),
    );
    agg.record(event);

    let mut out = vec![];
    agg.flush_event_time_buckets(&mut out, true);
    assert_eq!(
        out.len(),
        1,
        "realistic timestamp must record under boundary max_future_ms",
    );
}

/// With `max_future_ms = 0` arbitrary future timestamps are accepted, so a
/// metric near `DateTime::<Utc>::MAX_UTC` can produce a `bucket_key` close
/// to `i64::MAX`. The flush eligibility predicate adds `bucket_key +
/// interval_ms + grace_ms` in i64; plain `+` would either panic
/// (overflow-checked builds) or wrap negative -- which makes the cutoff
/// `<= now_ms` so the bucket flushes immediately and the watermark
/// advances to ~`i64::MAX`, after which every normal event is rejected
/// as late. Saturating arithmetic must instead park the cutoff at
/// `i64::MAX`, leaving the far-future bucket open until `force`.
#[test]
fn event_time_far_future_bucket_does_not_overflow_flush_cutoff() {
    let mut agg = Aggregate::new(&event_time_config_with(1000, AggregationMode::Auto, |et| {
        et.allowed_lateness_ms = i64::MAX as u64;
        et.max_future_ms = 0;
    }))
    .unwrap();

    let far_future = DateTime::<Utc>::MAX_UTC;
    let event = make_metric_with_timestamp(
        "far_future_counter",
        MetricKind::Incremental,
        MetricValue::Counter { value: 1.0 },
        far_future,
    );
    agg.record(event);

    // Non-force flush: the bucket must stay parked because its saturated
    // cutoff is `i64::MAX`, which `now_ms` never reaches.
    let mut out = vec![];
    agg.flush_event_time_buckets(&mut out, false);
    assert!(
        out.is_empty(),
        "a far-future bucket must not be eligible for non-forced flush",
    );
    assert_eq!(
        agg.event_time_buckets.len(),
        1,
        "the bucket must remain open after a non-forced flush",
    );
    assert!(
        agg.watermark.is_none(),
        "the watermark must not advance for a non-forced flush of an \
         ineligible bucket (advancing it to i64::MAX would drop every \
         subsequent normal event as late)",
    );

    // Force flush still drains it (shutdown / topology reload path).
    agg.flush_event_time_buckets(&mut out, true);
    assert_eq!(
        out.len(),
        1,
        "force flush must drain the far-future bucket so in-flight \
         events are not silently dropped on shutdown",
    );
}

/// Missing-timestamp fallback (`missing_timestamp = use_system_time`) must
/// accept and store the event, and must reuse one wall-clock instant for
/// synthesis and cutoff checks. With `allowed_lateness_ms = 0`, a second
/// `Utc::now()` read one millisecond after the bucket end would otherwise
/// reject an event whose synthesized timestamp still lies inside the bucket.
#[test]
fn event_time_missing_timestamp_fallback_reuses_now_for_cutoff() {
    let interval_ms = 1_000_u64;
    let interval_i64 = interval_ms as i64;

    let mut drop_agg =
        Aggregate::new(&event_time_config(interval_ms, AggregationMode::Auto)).unwrap();
    assert!(
        drop_agg
            .record(make_metric(
                "missing_timestamp_drop",
                MetricKind::Incremental,
                MetricValue::Counter { value: 1.0 },
            ))
            .is_none()
    );
    assert!(drop_agg.event_time_buckets.is_empty());

    let agg = Aggregate::new(&event_time_config_with(
        interval_ms,
        AggregationMode::Auto,
        |et| {
            et.missing_timestamp = MissingTimestamp::UseSystemTime;
        },
    ))
    .unwrap();

    let now_ms = Utc::now().timestamp_millis();
    let bucket_key = now_ms.div_euclid(interval_i64).saturating_mul(interval_i64);
    let last_ms_in_bucket = bucket_key + interval_i64 - 1;

    assert!(
        !agg.is_past_bucket_cutoff(bucket_key, last_ms_in_bucket),
        "same-instant cutoff must accept timestamps still inside the bucket"
    );
    assert!(
        agg.is_past_bucket_cutoff(bucket_key, last_ms_in_bucket + 1),
        "a later clock read crosses the bucket boundary — the race this fix avoids"
    );

    let mut agg = agg;
    let event = make_metric(
        "counter_no_ts",
        MetricKind::Incremental,
        MetricValue::Counter { value: 1.0 },
    );
    assert!(
        agg.record(event).is_none(),
        "missing-timestamp fallback must be bucketed, not dropped by a later cutoff read"
    );
    assert_eq!(
        agg.event_time_buckets.len(),
        1,
        "fallback event must land in the current bucket"
    );
}

#[test]
fn event_time_passthrough_skips_timestamp_gating_and_watermark_updates() {
    let interval_ms = 10_000;
    let mut agg = Aggregate::new(&event_time_config_with(
        interval_ms,
        AggregationMode::Mean,
        |et| et.max_future_ms = 600_000,
    ))
    .unwrap();

    let passthrough = make_metric(
        "passthrough",
        MetricKind::Incremental,
        MetricValue::Counter { value: 1.0 },
    );
    assert!(agg.record(passthrough).is_some());
    assert!(agg.event_time_buckets.is_empty());
    assert!(agg.event_time_multi_buckets.is_empty());

    let mut out = vec![];
    agg.flush_into(&mut out);
    assert!(out.is_empty());
    assert!(agg.watermark.is_none());

    agg.record(make_metric_with_timestamp(
        "aggregated",
        MetricKind::Absolute,
        MetricValue::Gauge { value: 42.0 },
        event_time_bucket_timestamp(interval_ms, 1),
    ));
    agg.flush_final(&mut out);
    assert_eq!(out.len(), 1);
}

#[test]
fn event_time_auto_handles_kind_switch_in_both_directions() {
    let interval_ms = 10_000_u64;
    let base_time = open_bucket_timestamp(interval_ms);

    // Direction 1: Incremental stored first, then Absolute with an *older* event
    // timestamp arrives. Absolute must win (matching system-time Auto) rather than
    // being silently dropped by timestamp-only comparison — this is the Absolute
    // arm's kind-mismatch branch, which already merges metadata unconditionally.
    {
        let mut agg =
            Aggregate::new(&event_time_config(interval_ms, AggregationMode::Auto)).unwrap();
        let ts_older = base_time + chrono::Duration::milliseconds(100);
        let ts_newer = base_time + chrono::Duration::milliseconds(200);
        let (incremental_batch, mut incremental_receiver) = BatchNotifier::new_with_receiver();
        let (absolute_batch, mut absolute_receiver) = BatchNotifier::new_with_receiver();

        agg.record(
            make_metric_with_timestamp(
                "the-thing",
                MetricKind::Incremental,
                MetricValue::Counter { value: 10.0 },
                ts_newer,
            )
            .with_batch_notifier(&incremental_batch),
        );
        agg.record(
            make_metric_with_timestamp(
                "the-thing",
                MetricKind::Absolute,
                MetricValue::Counter { value: 99.0 },
                ts_older,
            )
            .with_batch_notifier(&absolute_batch),
        );
        drop((incremental_batch, absolute_batch));

        let mut out = vec![];
        agg.flush_final(&mut out);
        assert_eq!(out.len(), 1);
        let metric = out[0].as_metric();
        assert_eq!(metric.kind(), MetricKind::Absolute);
        if let MetricValue::Counter { value } = metric.value() {
            assert_eq!(*value, 99.0);
        } else {
            panic!("expected absolute counter");
        }
        assert!(incremental_receiver.try_recv().is_err());
        assert!(absolute_receiver.try_recv().is_err());
        out[0].metadata().update_status(EventStatus::Delivered);
        drop(out);
        assert_eq!(
            incremental_receiver.try_recv(),
            Ok(BatchStatus::Delivered),
            "superseded incremental sample's finalizers must not be discarded"
        );
        assert_eq!(
            absolute_receiver.try_recv(),
            Ok(BatchStatus::Delivered),
            "retained absolute sample's finalizers must be delivered"
        );
    }

    // Direction 2 (mirror image): Absolute stored first, then Incremental arrives
    // for the same series/bucket. This is `record_sum_in_map`'s kind-mismatch
    // branch, which must also merge the superseded Absolute sample's metadata
    // instead of discarding it when it replaces the tuple.
    {
        let mut agg =
            Aggregate::new(&event_time_config(interval_ms, AggregationMode::Auto)).unwrap();
        let (absolute_batch, mut absolute_receiver) = BatchNotifier::new_with_receiver();
        let (incremental_batch, mut incremental_receiver) = BatchNotifier::new_with_receiver();

        agg.record(
            make_metric_with_timestamp(
                "the-thing",
                MetricKind::Absolute,
                MetricValue::Counter { value: 42.0 },
                base_time,
            )
            .with_batch_notifier(&absolute_batch),
        );
        agg.record(
            make_metric_with_timestamp(
                "the-thing",
                MetricKind::Incremental,
                MetricValue::Counter { value: 1.0 },
                base_time + chrono::Duration::milliseconds(100),
            )
            .with_batch_notifier(&incremental_batch),
        );
        drop((absolute_batch, incremental_batch));

        let mut out = vec![];
        agg.flush_final(&mut out);
        assert_eq!(out.len(), 1);
        let metric = out[0].as_metric();
        assert_eq!(metric.kind(), MetricKind::Incremental);
        if let MetricValue::Counter { value } = metric.value() {
            assert_eq!(*value, 1.0);
        } else {
            panic!("expected incremental counter");
        }
        // Neither finalizer may have resolved yet — if the superseded absolute
        // sample's metadata was dropped instead of merged, its `EventFinalizer`
        // would already have completed here (defaulting to `Delivered`) instead
        // of waiting on the emitted metric's own status update below.
        assert!(incremental_receiver.try_recv().is_err());
        assert!(
            absolute_receiver.try_recv().is_err(),
            "absolute sample's finalizer resolved prematurely (metadata was dropped, not merged)"
        );
        out[0].metadata().update_status(EventStatus::Delivered);
        drop(out);
        assert_eq!(
            incremental_receiver.try_recv(),
            Ok(BatchStatus::Delivered),
            "retained incremental sample's finalizers must be delivered"
        );
        assert_eq!(
            absolute_receiver.try_recv(),
            Ok(BatchStatus::Delivered),
            "superseded absolute sample's finalizers must not be discarded"
        );
    }
}

#[test]
fn event_time_metadata_is_merged_and_diff_finalizers_are_released() {
    let interval_ms = 10_000;
    let base_time = open_bucket_timestamp(interval_ms);

    // Latest/Auto must preserve finalizers from both retained and discarded samples.
    let mut latest =
        Aggregate::new(&event_time_config(interval_ms, AggregationMode::Auto)).unwrap();
    let (newer_batch, mut newer_receiver) = BatchNotifier::new_with_receiver();
    let (older_batch, mut older_receiver) = BatchNotifier::new_with_receiver();
    let newer = make_metric_with_timestamp(
        "latest",
        MetricKind::Absolute,
        MetricValue::Gauge { value: 2.0 },
        base_time + chrono::Duration::milliseconds(200),
    )
    .with_batch_notifier(&newer_batch);
    let older = make_metric_with_timestamp(
        "latest",
        MetricKind::Absolute,
        MetricValue::Gauge { value: 1.0 },
        base_time + chrono::Duration::milliseconds(100),
    )
    .with_batch_notifier(&older_batch);
    drop((newer_batch, older_batch));

    latest.record(newer);
    latest.record(older);
    let mut out = vec![];
    latest.flush_final(&mut out);
    assert!(newer_receiver.try_recv().is_err());
    assert!(older_receiver.try_recv().is_err());
    out[0].metadata().update_status(EventStatus::Delivered);
    drop(out);
    assert_eq!(newer_receiver.try_recv(), Ok(BatchStatus::Delivered));
    assert_eq!(older_receiver.try_recv(), Ok(BatchStatus::Delivered));

    // Diff retains prior metric data, but must not retain metadata/finalizers.
    let mut diff = Aggregate::new(&event_time_config(interval_ms, AggregationMode::Diff)).unwrap();
    let (diff_batch, mut diff_receiver) = BatchNotifier::new_with_receiver();
    let event = make_metric_with_timestamp(
        "diff",
        MetricKind::Absolute,
        MetricValue::Gauge { value: 1.0 },
        base_time,
    )
    .with_batch_notifier(&diff_batch);
    drop(diff_batch);

    diff.record(event);
    let mut out = vec![];
    diff.flush_final(&mut out);
    out[0].metadata().update_status(EventStatus::Delivered);
    drop(out);
    assert_eq!(
        diff_receiver.try_recv(),
        Ok(BatchStatus::Delivered),
        "Diff retention must not keep finalizers alive"
    );
    assert_eq!(diff.event_time_prev_buckets.len(), 1);

    // Max/Min must merge metadata from every sample, including the one that
    // loses the comparison and is not retained.
    for mode in [AggregationMode::Max, AggregationMode::Min] {
        let mut agg = Aggregate::new(&event_time_config(interval_ms, mode)).unwrap();
        let (winner_batch, mut winner_receiver) = BatchNotifier::new_with_receiver();
        let (loser_batch, mut loser_receiver) = BatchNotifier::new_with_receiver();

        let winning_value = if mode == AggregationMode::Max {
            99.0
        } else {
            1.0
        };
        let losing_value = if mode == AggregationMode::Max {
            2.0
        } else {
            50.0
        };

        let winner = make_metric_with_timestamp(
            "extremum",
            MetricKind::Absolute,
            MetricValue::Gauge {
                value: winning_value,
            },
            base_time,
        )
        .with_batch_notifier(&winner_batch);
        let loser = make_metric_with_timestamp(
            "extremum",
            MetricKind::Absolute,
            MetricValue::Gauge {
                value: losing_value,
            },
            base_time + chrono::Duration::milliseconds(100),
        )
        .with_batch_notifier(&loser_batch);
        drop((winner_batch, loser_batch));

        agg.record(winner);
        agg.record(loser);
        let mut out = vec![];
        agg.flush_final(&mut out);
        assert_eq!(out.len(), 1);
        if let MetricValue::Gauge { value } = out[0].as_metric().value() {
            assert_eq!(
                *value, winning_value,
                "{mode:?} must retain the winning value"
            );
        } else {
            panic!("expected gauge value");
        }
        out[0].metadata().update_status(EventStatus::Delivered);
        drop(out);
        assert_eq!(
            winner_receiver.try_recv(),
            Ok(BatchStatus::Delivered),
            "{mode:?}: winning sample's finalizers must be delivered"
        );
        assert_eq!(
            loser_receiver.try_recv(),
            Ok(BatchStatus::Delivered),
            "{mode:?}: losing sample's finalizers must not be discarded"
        );
    }
}
