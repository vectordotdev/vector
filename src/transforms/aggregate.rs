use std::{
    collections::{hash_map::Entry, HashMap},
    pin::Pin,
    time::Duration,
};

use async_stream::stream;
use futures::{Stream, StreamExt};
use vector_lib::{config::LogNamespace, event::MetricValue};
use vector_lib::{
    configurable::configurable_component,
    event::metric::{Metric, MetricData, MetricKind, MetricSeries},
};

use crate::{
    config::{DataType, Input, OutputId, TransformConfig, TransformContext, TransformOutput},
    event::{Event, EventMetadata},
    internal_events::{AggregateEventRecorded, AggregateFlushed, AggregateUpdateFailed},
    schema,
    transforms::{TaskTransform, Transform},
};

/// Configuration for the `aggregate` transform.
#[configurable_component(transform("aggregate", "Aggregate metrics passing through a topology."))]
#[derive(Clone, Debug, Default)]
#[serde(deny_unknown_fields)]
pub struct AggregateConfig {
    /// The interval between flushes, in milliseconds.
    ///
    /// During this time frame, metrics (beta) with the same series data (name, namespace, tags, and so on) are aggregated.
    #[serde(default = "default_interval_ms")]
    #[configurable(metadata(docs::human_name = "Flush Interval"))]
    pub interval_ms: u64,
    /// Function to use for aggregation.
    ///
    /// Some of the functions may only function on incremental and some only on absolute metrics.
    #[serde(default = "default_mode")]
    #[configurable(derived)]
    pub mode: AggregationMode,
}

#[configurable_component]
#[derive(Clone, Debug, Default)]
#[configurable(description = "The aggregation mode to use.")]
pub enum AggregationMode {
    /// Default mode. Sums incremental metrics and uses the latest value for absolute metrics.
    #[default]
    Auto,

    /// Sums incremental metrics, ignores absolute
    Sum,

    /// Returns the latest value for absolute metrics, ignores incremental
    Latest,

    /// Counts metrics for incremental and absolute metrics
    Count,

    /// Returns difference between latest value for absolute, ignores incremental
    Diff,

    /// Max value of absolute metric, ignores incremental
    Max,

    /// Min value of absolute metric, ignores incremental
    Min,

    /// Mean value of absolute metric, ignores incremental
    Mean,

    /// Stdev value of absolute metric, ignores incremental
    Stdev,
}

const fn default_mode() -> AggregationMode {
    AggregationMode::Auto
}

const fn default_interval_ms() -> u64 {
    10 * 1000
}

impl_generate_config_from_default!(AggregateConfig);

#[async_trait::async_trait]
#[typetag::serde(name = "aggregate")]
impl TransformConfig for AggregateConfig {
    async fn build(&self, _context: &TransformContext) -> crate::Result<Transform> {
        Aggregate::new(self).map(Transform::event_task)
    }

    fn input(&self) -> Input {
        Input::metric()
    }

    fn outputs(
        &self,
        _: vector_lib::enrichment::TableRegistry,
        _: vector_lib::vrl_cache::VrlCacheRegistry,
        _: &[(OutputId, schema::Definition)],
        _: LogNamespace,
    ) -> Vec<TransformOutput> {
        vec![TransformOutput::new(DataType::Metric, HashMap::new())]
    }
}

type MetricEntry = (MetricData, EventMetadata);

#[derive(Debug)]
pub struct Aggregate {
    interval: Duration,
    map: HashMap<MetricSeries, MetricEntry>,
    prev_map: HashMap<MetricSeries, MetricEntry>,
    multi_map: HashMap<MetricSeries, Vec<MetricEntry>>,
    mode: AggregationMode,
}

impl Aggregate {
    pub fn new(config: &AggregateConfig) -> crate::Result<Self> {
        Ok(Self {
            interval: Duration::from_millis(config.interval_ms),
            map: Default::default(),
            prev_map: Default::default(),
            multi_map: Default::default(),
            mode: config.mode.clone(),
        })
    }

    fn record(&mut self, event: Event) {
        let (series, data, metadata) = event.into_metric().into_parts();

        match self.mode {
            AggregationMode::Auto => match data.kind {
                MetricKind::Incremental => self.record_sum(series, data, metadata),
                MetricKind::Absolute => {
                    self.map.insert(series, (data, metadata));
                }
            },
            AggregationMode::Sum => self.record_sum(series, data, metadata),
            AggregationMode::Latest | AggregationMode::Diff => match data.kind {
                MetricKind::Incremental => (),
                MetricKind::Absolute => {
                    self.map.insert(series, (data, metadata));
                }
            },
            AggregationMode::Count => self.record_count(series, data, metadata),
            AggregationMode::Max | AggregationMode::Min => {
                self.record_comparison(series, data, metadata)
            }
            AggregationMode::Mean | AggregationMode::Stdev => match data.kind {
                MetricKind::Incremental => (),
                MetricKind::Absolute => {
                    if matches!(data.value, MetricValue::Gauge { value: _ }) {
                        match self.multi_map.entry(series) {
                            Entry::Occupied(mut entry) => {
                                let existing = entry.get_mut();
                                existing.push((data, metadata));
                            }
                            Entry::Vacant(entry) => {
                                entry.insert(vec![(data, metadata)]);
                            }
                        }
                    }
                }
            },
        }

        emit!(AggregateEventRecorded);
    }

    fn record_count(
        &mut self,
        series: MetricSeries,
        mut data: MetricData,
        metadata: EventMetadata,
    ) {
        let mut count_data = data.clone();
        let existing = self.map.entry(series).or_insert_with(|| {
            *data.value_mut() = MetricValue::Counter { value: 0f64 };
            (data.clone(), metadata.clone())
        });
        *count_data.value_mut() = MetricValue::Counter { value: 1f64 };
        if existing.0.kind == data.kind && existing.0.update(&count_data) {
            existing.1.merge(metadata);
        } else {
            emit!(AggregateUpdateFailed);
        }
    }

    fn record_sum(&mut self, series: MetricSeries, data: MetricData, metadata: EventMetadata) {
        match data.kind {
            MetricKind::Incremental => match self.map.entry(series) {
                Entry::Occupied(mut entry) => {
                    let existing = entry.get_mut();
                    // In order to update (add) the new and old kind's must match
                    if existing.0.kind == data.kind && existing.0.update(&data) {
                        existing.1.merge(metadata);
                    } else {
                        emit!(AggregateUpdateFailed);
                        *existing = (data, metadata);
                    }
                }
                Entry::Vacant(entry) => {
                    entry.insert((data, metadata));
                }
            },
            MetricKind::Absolute => {}
        }
    }

    fn record_comparison(
        &mut self,
        series: MetricSeries,
        data: MetricData,
        metadata: EventMetadata,
    ) {
        match data.kind {
            MetricKind::Incremental => (),
            MetricKind::Absolute => match self.map.entry(series) {
                Entry::Occupied(mut entry) => {
                    let existing = entry.get_mut();
                    // In order to update (add) the new and old kind's must match
                    if existing.0.kind == data.kind {
                        if let MetricValue::Gauge {
                            value: existing_value,
                        } = existing.0.value()
                        {
                            if let MetricValue::Gauge { value: new_value } = data.value() {
                                let should_update = match self.mode {
                                    AggregationMode::Max => new_value > existing_value,
                                    AggregationMode::Min => new_value < existing_value,
                                    _ => false,
                                };
                                if should_update {
                                    *existing = (data, metadata);
                                }
                            }
                        }
                    } else {
                        emit!(AggregateUpdateFailed);
                        *existing = (data, metadata);
                    }
                }
                Entry::Vacant(entry) => {
                    entry.insert((data, metadata));
                }
            },
        }
    }

    fn flush_into(&mut self, output: &mut Vec<Event>) {
        let map = std::mem::take(&mut self.map);
        for (series, entry) in map.clone().into_iter() {
            let mut metric = Metric::from_parts(series, entry.0, entry.1);
            if matches!(self.mode, AggregationMode::Diff) {
                if let Some(prev_entry) = self.prev_map.get(metric.series()) {
                    if metric.data().kind == prev_entry.0.kind && !metric.subtract(&prev_entry.0) {
                        emit!(AggregateUpdateFailed);
                    }
                }
            }
            output.push(Event::Metric(metric));
        }

        let multi_map = std::mem::take(&mut self.multi_map);
        'outer: for (series, entries) in multi_map.into_iter() {
            if entries.is_empty() {
                continue;
            }

            let (mut final_sum, mut final_metadata) = entries.first().unwrap().clone();
            for (data, metadata) in entries.iter().skip(1) {
                if !final_sum.update(data) {
                    // Incompatible types, skip this metric
                    emit!(AggregateUpdateFailed);
                    continue 'outer;
                }
                final_metadata.merge(metadata.clone());
            }

            let final_mean_value = if let MetricValue::Gauge { value } = final_sum.value_mut() {
                // Entries are not empty so this is safe.
                *value /= entries.len() as f64;
                *value
            } else {
                0.0
            };

            let final_mean = final_sum.clone();
            match self.mode {
                AggregationMode::Mean => {
                    let metric = Metric::from_parts(series, final_mean, final_metadata);
                    output.push(Event::Metric(metric));
                }
                AggregationMode::Stdev => {
                    let variance = entries
                        .iter()
                        .filter_map(|(data, _)| {
                            if let MetricValue::Gauge { value } = data.value() {
                                let diff = final_mean_value - value;
                                Some(diff * diff)
                            } else {
                                None
                            }
                        })
                        .sum::<f64>()
                        / entries.len() as f64;
                    let mut final_stdev = final_mean;
                    if let MetricValue::Gauge { value } = final_stdev.value_mut() {
                        *value = variance.sqrt()
                    }
                    let metric = Metric::from_parts(series, final_stdev, final_metadata);
                    output.push(Event::Metric(metric));
                }
                _ => (),
            }
        }

        self.prev_map = map;
        emit!(AggregateFlushed);
    }
}

impl TaskTransform<Event> for Aggregate {
    fn transform(
        mut self: Box<Self>,
        mut input_rx: Pin<Box<dyn Stream<Item = Event> + Send>>,
    ) -> Pin<Box<dyn Stream<Item = Event> + Send>>
    where
        Self: 'static,
    {
        let mut flush_stream = tokio::time::interval(self.interval);

        Box::pin(stream! {
            let mut output = Vec::new();
            let mut done = false;
            while !done {
                tokio::select! {
                    _ = flush_stream.tick() => {
                        self.flush_into(&mut output);
                    },
                    maybe_event = input_rx.next() => {
                        match maybe_event {
                            None => {
                                self.flush_into(&mut output);
                                done = true;
                            }
                            Some(event) => self.record(event),
                        }
                    }
                };
                for event in output.drain(..) {
                    yield event;
                }
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeSet, sync::Arc, task::Poll};

    use futures::stream;
    use tokio::sync::mpsc;
    use tokio_stream::wrappers::ReceiverStream;
    use vector_lib::config::ComponentKey;
    use vrl::value::Kind;

    use super::*;
    use crate::schema::Definition;
    use crate::{
        event::{
            metric::{MetricKind, MetricValue},
            Event, Metric,
        },
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
        event.metadata_mut().set_schema_definition(&Arc::new(
            Definition::new_with_default_metadata(Kind::any_object(), [LogNamespace::Legacy]),
        ));

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
            agg.record(gauge);
        }
        let mut out = vec![];
        agg.flush_into(&mut out);
        assert_eq!(1, out.len());
        assert_eq!(&stdev_result, &out[0]);
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
        let agg = toml::from_str::<AggregateConfig>(
            r#"
interval_ms = 999999
"#,
        )
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
        let transform_config = toml::from_str::<AggregateConfig>("").unwrap();

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
                if let Some(event) = out.next().await {
                    match event.as_metric().series().name.name.as_str() {
                        "counter_a" => assert_eq!(counter_a_summed, event),
                        "gauge_a" => assert_eq!(gauge_a_2, event),
                        _ => panic!("Unexpected metric name in aggregate output"),
                    };
                    count += 1;
                } else {
                    panic!("Unexpectedly received None in output stream");
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
}
