use super::{AggregateConfig, AggregationMode};

use std::{
    collections::{HashMap, hash_map::Entry},
    pin::Pin,
    time::Duration,
};

use async_stream::stream;
use futures::{Stream, StreamExt};
use vector_lib::event::{
    MetricValue,
    metric::{Metric, MetricData, MetricKind, MetricSeries},
};

use crate::{
    event::{Event, EventMetadata},
    internal_events::{AggregateEventRecorded, AggregateFlushed, AggregateUpdateFailed},
    transforms::TaskTransform,
};

#[derive(Clone, Debug, Default, PartialEq)]
enum InnerMode {
    /// Default mode. Sums incremental metrics and uses the latest value for absolute metrics.
    #[default]
    Auto,

    /// Sums incremental metrics; absolute metrics pass through unchanged.
    Sum,

    /// Returns the latest value for absolute metrics; incremental metrics pass through unchanged.
    Latest,

    /// Counts metrics for incremental and absolute metrics
    Count,

    /// Returns difference between latest value for absolute; incremental metrics pass through unchanged.
    Diff {
        prev_map: HashMap<MetricSeries, MetricEntry>,
    },

    /// Max value of absolute metric; incremental metrics pass through unchanged.
    Max,

    /// Min value of absolute metric; incremental metrics pass through unchanged.
    Min,

    /// Mean value of absolute metric; incremental metrics pass through unchanged.
    Mean {
        multi_map: HashMap<MetricSeries, Vec<MetricEntry>>,
    },

    /// Stdev value of absolute metric; incremental metrics pass through unchanged.
    Stdev {
        multi_map: HashMap<MetricSeries, Vec<MetricEntry>>,
    },
}

impl From<AggregationMode> for InnerMode {
    fn from(value: AggregationMode) -> Self {
        match value {
            AggregationMode::Auto => InnerMode::Auto,
            AggregationMode::Sum => InnerMode::Sum,
            AggregationMode::Latest => InnerMode::Latest,
            AggregationMode::Count => InnerMode::Count,
            AggregationMode::Diff => InnerMode::Diff {
                prev_map: HashMap::default(),
            },
            AggregationMode::Max => InnerMode::Max,
            AggregationMode::Min => InnerMode::Min,
            AggregationMode::Mean => InnerMode::Mean {
                multi_map: HashMap::default(),
            },
            AggregationMode::Stdev => InnerMode::Stdev {
                multi_map: HashMap::default(),
            },
        }
    }
}

type MetricEntry = (MetricData, EventMetadata);

#[derive(Debug)]
pub struct Aggregate {
    interval: Duration,
    map: HashMap<MetricSeries, MetricEntry>,
    mode: InnerMode,
}

impl Aggregate {
    pub fn new(config: &AggregateConfig) -> crate::Result<Self> {
        Ok(Self {
            interval: Duration::from_millis(config.interval_ms),
            map: Default::default(),
            mode: config.mode.into(),
        })
    }

    pub fn record(&mut self, event: Event) -> Option<Event> {
        let (series, data, metadata) = event.into_metric().into_parts();

        match (&mut self.mode, data.kind) {
            (InnerMode::Sum, MetricKind::Absolute)
            | (InnerMode::Latest | InnerMode::Diff { .. }, MetricKind::Incremental)
            | (InnerMode::Max | InnerMode::Min, MetricKind::Incremental)
            | (InnerMode::Mean { .. } | InnerMode::Stdev { .. }, MetricKind::Incremental) => {
                return Some(Event::Metric(Metric::from_parts(series, data, metadata)));
            }
            (InnerMode::Auto | InnerMode::Sum, MetricKind::Incremental) => {
                self.record_sum(series, data, metadata);
            }
            (InnerMode::Auto, MetricKind::Absolute)
            | (InnerMode::Latest | InnerMode::Diff { .. }, MetricKind::Absolute) => {
                self.map.insert(series, (data, metadata));
            }
            (InnerMode::Count, _) => {
                self.record_count(series, data, metadata);
            }
            (InnerMode::Max | InnerMode::Min, MetricKind::Absolute) => {
                self.record_comparison(series, data, metadata);
            }
            (
                InnerMode::Mean { multi_map } | InnerMode::Stdev { multi_map },
                MetricKind::Absolute,
            ) => {
                if matches!(data.value, MetricValue::Gauge { value: _ }) {
                    match multi_map.entry(series) {
                        Entry::Occupied(mut entry) => entry.get_mut().push((data, metadata)),
                        Entry::Vacant(entry) => {
                            entry.insert(vec![(data, metadata)]);
                        }
                    }
                }
            }
        }
        emit!(AggregateEventRecorded);
        None
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
        match self.map.entry(series) {
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
        }
    }

    fn record_comparison(
        &mut self,
        series: MetricSeries,
        data: MetricData,
        metadata: EventMetadata,
    ) {
        match self.map.entry(series) {
            Entry::Occupied(mut entry) => {
                let existing = entry.get_mut();
                // In order to update (add) the new and old kind's must match
                if existing.0.kind == data.kind {
                    if let MetricValue::Gauge {
                        value: existing_value,
                    } = existing.0.value()
                        && let MetricValue::Gauge { value: new_value } = data.value()
                    {
                        let should_update = match self.mode {
                            InnerMode::Max => new_value > existing_value,
                            InnerMode::Min => new_value < existing_value,
                            _ => false,
                        };
                        if should_update {
                            *existing = (data, metadata);
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
        }
    }

    pub fn flush_into(&mut self, output: &mut Vec<Event>) {
        let map = std::mem::take(&mut self.map);
        for (series, entry) in map.clone().into_iter() {
            let mut metric = Metric::from_parts(series, entry.0, entry.1);
            if let InnerMode::Diff { prev_map } = &self.mode
                && let Some(prev_entry) = prev_map.get(metric.series())
                && metric.data().kind == prev_entry.0.kind
                && !metric.subtract(&prev_entry.0)
            {
                emit!(AggregateUpdateFailed);
            }
            output.push(Event::Metric(metric));
        }

        let multi_map = match &mut self.mode {
            InnerMode::Mean { multi_map } | InnerMode::Stdev { multi_map } => {
                std::mem::take(multi_map)
            }
            _ => HashMap::default(),
        };

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
                InnerMode::Mean { .. } => {
                    let metric = Metric::from_parts(series, final_mean, final_metadata);
                    output.push(Event::Metric(metric));
                }
                InnerMode::Stdev { .. } => {
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

        if let InnerMode::Diff { prev_map } = &mut self.mode {
            *prev_map = map;
        }
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
                            Some(event) => {
                                if let Some(passthrough) = self.record(event) {
                                    output.push(passthrough);
                                }
                            }
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
