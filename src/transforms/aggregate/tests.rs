use std::{collections::BTreeSet, sync::Arc, task::Poll, time::Duration};

use futures::{StreamExt, stream};
use indoc::indoc;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use vector_lib::config::{ComponentKey, LogNamespace};
use vrl::value::Kind;

use super::*;
use crate::{
    config::{OutputId, TransformConfig, TransformContext},
    event::{
        Event, Metric,
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

#[test]
fn incremental_auto() {
    let mut agg = Aggregate::new(&AggregateConfig {
        interval_ms: 1000_u64,
        mode: AggregationMode::Auto,
    })
    .unwrap();

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
    assert_eq!(agg.record(counter_a_1.clone()), None);
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
    assert_eq!(agg.record(counter_a_1.clone()), None);
    assert_eq!(agg.record(counter_a_2), None);
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
    assert_eq!(agg.record(counter_a_1.clone()), None);
    assert_eq!(agg.record(counter_b_1.clone()), None);
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
fn absolute_auto() {
    let mut agg = Aggregate::new(&AggregateConfig {
        interval_ms: 1000_u64,
        mode: AggregationMode::Auto,
    })
    .unwrap();

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
    assert_eq!(agg.record(gauge_a_1.clone()), None);
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
    assert_eq!(agg.record(gauge_a_1.clone()), None);
    assert_eq!(agg.record(gauge_a_2.clone()), None);
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
    assert_eq!(agg.record(gauge_a_1.clone()), None);
    assert_eq!(agg.record(gauge_b_1.clone()), None);
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
    let mut agg = Aggregate::new(&AggregateConfig {
        interval_ms: 1000_u64,
        mode: AggregationMode::Count,
    })
    .unwrap();

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
    assert_eq!(agg.record(gauge_a_1.clone()), None);
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
    assert_eq!(agg.record(gauge_a_1.clone()), None);
    assert_eq!(agg.record(gauge_a_2.clone()), None);
    out.clear();
    agg.flush_into(&mut out);
    assert_eq!(1, out.len());
    assert_eq!(&result_count_2, &out[0]);
}

#[test]
fn absolute_max() {
    let mut agg = Aggregate::new(&AggregateConfig {
        interval_ms: 1000_u64,
        mode: AggregationMode::Max,
    })
    .unwrap();

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
    assert_eq!(agg.record(gauge_a_2.clone()), None);
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
    assert_eq!(agg.record(gauge_a_1.clone()), None);
    assert_eq!(agg.record(gauge_a_2.clone()), None);
    out.clear();
    agg.flush_into(&mut out);
    assert_eq!(1, out.len());
    assert_eq!(&gauge_a_1, &out[0]);
}

#[test]
fn absolute_min() {
    let mut agg = Aggregate::new(&AggregateConfig {
        interval_ms: 1000_u64,
        mode: AggregationMode::Min,
    })
    .unwrap();

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
    assert_eq!(agg.record(gauge_a_2.clone()), None);
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
    assert_eq!(agg.record(gauge_a_1.clone()), None);
    assert_eq!(agg.record(gauge_a_2.clone()), None);
    out.clear();
    agg.flush_into(&mut out);
    assert_eq!(1, out.len());
    assert_eq!(&gauge_a_1, &out[0]);
}

#[test]
fn absolute_diff() {
    let mut agg = Aggregate::new(&AggregateConfig {
        interval_ms: 1000_u64,
        mode: AggregationMode::Diff,
    })
    .unwrap();

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
    assert_eq!(agg.record(gauge_a_2.clone()), None);
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
    assert_eq!(agg.record(gauge_a_1.clone()), None);
    out.clear();
    agg.flush_into(&mut out);
    assert_eq!(1, out.len());
    assert_eq!(&gauge_a_1, &out[0]);

    assert_eq!(agg.record(gauge_a_2.clone()), None);
    out.clear();
    agg.flush_into(&mut out);
    assert_eq!(1, out.len());
    assert_eq!(&result, &out[0]);
}

#[test]
fn absolute_diff_conflicting_type() {
    let mut agg = Aggregate::new(&AggregateConfig {
        interval_ms: 1000_u64,
        mode: AggregationMode::Diff,
    })
    .unwrap();

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
    assert_eq!(agg.record(gauge_a_1.clone()), None);
    out.clear();
    agg.flush_into(&mut out);
    assert_eq!(1, out.len());
    assert_eq!(&gauge_a_1, &out[0]);

    assert_eq!(agg.record(gauge_a_2.clone()), None);
    out.clear();
    agg.flush_into(&mut out);
    assert_eq!(1, out.len());
    // Due to incompatible results, the new value just overwrites the old one
    assert_eq!(&gauge_a_2, &out[0]);
}

#[test]
fn absolute_mean() {
    let mut agg = Aggregate::new(&AggregateConfig {
        interval_ms: 1000_u64,
        mode: AggregationMode::Mean,
    })
    .unwrap();

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
    assert_eq!(agg.record(gauge_a_2.clone()), None);
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
    assert_eq!(agg.record(gauge_a_1.clone()), None);
    assert_eq!(agg.record(gauge_a_2.clone()), None);
    assert_eq!(agg.record(gauge_a_3.clone()), None);
    out.clear();
    agg.flush_into(&mut out);
    assert_eq!(1, out.len());
    assert_eq!(&mean_result, &out[0]);
}

#[test]
fn absolute_stdev() {
    let mut agg = Aggregate::new(&AggregateConfig {
        interval_ms: 1000_u64,
        mode: AggregationMode::Stdev,
    })
    .unwrap();

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
        assert_eq!(agg.record(gauge), None);
    }
    let mut out = vec![];
    agg.flush_into(&mut out);
    assert_eq!(1, out.len());
    assert_eq!(&stdev_result, &out[0]);
}

#[test]
fn passes_through_ignored_kind() {
    // Sum mode aggregates incremental, passes through absolute without collapsing.
    let mut agg = Aggregate::new(&AggregateConfig {
        interval_ms: 1000_u64,
        mode: AggregationMode::Sum,
    })
    .unwrap();

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
fn conflicting_value_type() {
    let mut agg = Aggregate::new(&AggregateConfig {
        interval_ms: 1000_u64,
        mode: AggregationMode::Auto,
    })
    .unwrap();

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
    assert_eq!(agg.record(counter.clone()), None);
    // Another will "add" to it
    assert_eq!(agg.record(counter.clone()), None);
    // Then an set will replace it due to a failed update
    assert_eq!(agg.record(set.clone()), None);
    // Then a set union would be a noop
    assert_eq!(agg.record(set.clone()), None);
    let mut out = vec![];
    // We should flush 1 item counter
    agg.flush_into(&mut out);
    assert_eq!(1, out.len());
    assert_eq!(&set, &out[0]);

    // Start out with an set
    assert_eq!(agg.record(set.clone()), None);
    // Union with itself, a noop
    assert_eq!(agg.record(set), None);
    // Send an counter with the same name, will replace due to a failed update
    assert_eq!(agg.record(counter.clone()), None);
    // Send another counter will "add"
    assert_eq!(agg.record(counter), None);
    let mut out = vec![];
    // We should flush 1 item counter
    agg.flush_into(&mut out);
    assert_eq!(1, out.len());
    assert_eq!(&summed, &out[0]);
}

#[test]
fn conflicting_kinds() {
    let mut agg = Aggregate::new(&AggregateConfig {
        interval_ms: 1000_u64,
        mode: AggregationMode::Auto,
    })
    .unwrap();

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
    assert_eq!(agg.record(incremental.clone()), None);
    // Another will "add" to it
    assert_eq!(agg.record(incremental.clone()), None);
    // Then an absolute will replace it with a failed update
    assert_eq!(agg.record(absolute.clone()), None);
    // Then another absolute will replace it normally
    assert_eq!(agg.record(absolute.clone()), None);
    let mut out = vec![];
    // We should flush 1 item incremental
    agg.flush_into(&mut out);
    assert_eq!(1, out.len());
    assert_eq!(&absolute, &out[0]);

    // Start out with an absolute
    assert_eq!(agg.record(absolute.clone()), None);
    // Replace it normally
    assert_eq!(agg.record(absolute), None);
    // Send an incremental with the same name, will replace due to a failed update
    assert_eq!(agg.record(incremental.clone()), None);
    // Send another incremental will "add"
    assert_eq!(agg.record(incremental), None);
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
