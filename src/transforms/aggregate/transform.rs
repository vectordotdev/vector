use super::{AggregateConfig, AggregationMode};

use std::{
    collections::{BTreeMap, HashMap, hash_map::Entry},
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

pub(crate) type MetricEntry = (MetricData, EventMetadata);

pub(crate) type BucketKey = i64;

#[derive(Debug)]
pub struct Aggregate {
    interval: Duration,
    map: HashMap<MetricSeries, MetricEntry>,
    mode: InnerMode,
    pub(crate) event_time_buckets: BTreeMap<BucketKey, HashMap<MetricSeries, MetricEntry>>,
    /// Previous bucket *data* only (no `EventMetadata`) so Diff can subtract
    /// without retaining acknowledgement finalizers after emission.
    pub(crate) event_time_prev_buckets: BTreeMap<BucketKey, HashMap<MetricSeries, MetricData>>,
    pub(crate) event_time_multi_buckets:
        BTreeMap<BucketKey, HashMap<MetricSeries, Vec<MetricEntry>>>,
    pub(crate) watermark: Option<BucketKey>,
    pub(crate) config: AggregateConfig,
}

/// Upper bound for any millisecond-valued duration field that is later cast
/// to `i64` for use with `chrono::Duration` and bucket arithmetic. Values
/// above this would wrap when cast and silently produce negative durations,
/// which corrupts watermark and future-skew checks.
const MAX_DURATION_MS: u64 = i64::MAX as u64;

impl Aggregate {
    pub fn new(config: &AggregateConfig) -> crate::Result<Self> {
        if config.interval_ms == 0 {
            return Err("`interval_ms` must be greater than 0".into());
        }
        if config.interval_ms > MAX_DURATION_MS {
            return Err(format!(
                "`interval_ms` ({}) exceeds the maximum supported value of {} ms",
                config.interval_ms, MAX_DURATION_MS
            )
            .into());
        }
        if let Some(event_time) = &config.event_time {
            if event_time.max_future_ms > MAX_DURATION_MS {
                return Err(format!(
                    "`event_time.max_future_ms` ({}) exceeds the maximum supported value of {} ms",
                    event_time.max_future_ms, MAX_DURATION_MS
                )
                .into());
            }
            if event_time.allowed_lateness_ms > MAX_DURATION_MS {
                return Err(format!(
                    "`event_time.allowed_lateness_ms` ({}) exceeds the maximum supported value of {} ms",
                    event_time.allowed_lateness_ms, MAX_DURATION_MS
                )
                .into());
            }
        }

        Ok(Self {
            interval: Duration::from_millis(config.interval_ms),
            map: Default::default(),
            mode: config.mode.into(),
            event_time_buckets: Default::default(),
            event_time_prev_buckets: Default::default(),
            event_time_multi_buckets: Default::default(),
            watermark: None,
            config: *config,
        })
    }

    pub fn record(&mut self, event: Event) -> Option<Event> {
        let metric = event.into_metric();
        let timestamp = metric.timestamp();
        let (series, data, metadata) = metric.into_parts();

        if self.config.is_event_time() {
            return self.record_event_time(series, data, metadata, timestamp);
        }

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

    /// Returns `true` for the system-time passthrough arms in `record()` — metrics
    /// that are forwarded downstream immediately without being aggregated.
    pub(crate) const fn passes_through_unchanged(mode: AggregationMode, data: &MetricData) -> bool {
        matches!(
            (mode, data.kind),
            (AggregationMode::Sum, MetricKind::Absolute)
                | (AggregationMode::Latest, MetricKind::Incremental)
                | (AggregationMode::Diff, MetricKind::Incremental)
                | (AggregationMode::Max, MetricKind::Incremental)
                | (AggregationMode::Min, MetricKind::Incremental)
                | (AggregationMode::Mean, MetricKind::Incremental)
                | (AggregationMode::Stdev, MetricKind::Incremental)
        )
    }

    /// Returns `true` for metrics that system-time accepts but does not store or
    /// pass through — currently absolute non-gauge values in `Mean`/`Stdev`.
    pub(crate) const fn is_silently_ignored(mode: AggregationMode, data: &MetricData) -> bool {
        matches!(mode, AggregationMode::Mean | AggregationMode::Stdev)
            && matches!(data.kind, MetricKind::Absolute)
            && !matches!(data.value, MetricValue::Gauge { value: _ })
    }

    /// Returns `true` iff a record with the given `kind`/`value` would be
    /// stored under `mode`. Mirrors the per-mode filters in `record_sum`,
    /// `record_comparison`, the `Latest`/`Diff` Absolute-only path, and the
    /// `Mean`/`Stdev` Gauge check, so the (mode, kind, value) compatibility
    /// can be decided *before* a bucket entry is created.
    ///
    /// This matters because in event-time mode an empty bucket is still
    /// considered eligible to flush and would *advance the watermark*; we
    /// must therefore avoid materialising buckets for events that the mode
    /// would silently no-op on, otherwise a stray incompatible event could
    /// reject valid in-order events for earlier buckets.
    ///
    /// Passthrough and silently-ignored metrics are handled in `record()` before
    /// this is relevant; callers that reach `record_into_bucket` must already
    /// be storable (`debug_assert!` in tests).
    pub(crate) const fn will_be_stored(mode: AggregationMode, data: &MetricData) -> bool {
        match mode {
            // `Auto` stores both kinds (sum incremental, latest absolute).
            // `Count` stores both kinds; per-series kind mismatches surface
            // later via `AggregateUpdateFailed`, but the first event for a
            // series always lands so a bucket is never created spuriously.
            AggregationMode::Auto | AggregationMode::Count => true,
            AggregationMode::Sum => matches!(data.kind, MetricKind::Incremental),
            // `Latest`/`Diff` and `Max`/`Min` only act on absolute metrics.
            AggregationMode::Latest
            | AggregationMode::Diff
            | AggregationMode::Max
            | AggregationMode::Min => matches!(data.kind, MetricKind::Absolute),
            // `Mean`/`Stdev` only record absolute Gauges.
            AggregationMode::Mean | AggregationMode::Stdev => {
                matches!(data.kind, MetricKind::Absolute)
                    && matches!(data.value, MetricValue::Gauge { value: _ })
            }
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
        // Count mode counts every sample regardless of kind (a series may mix
        // Absolute and Incremental metrics), so — unlike Sum/Latest/Max/Min —
        // kind must not gate the update or mixed-kind series get undercounted.
        existing.1.merge(metadata);
        if !existing.0.update(&count_data) {
            emit!(AggregateUpdateFailed);
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
        if self.config.is_event_time() {
            self.flush_event_time_buckets(output, false);
        } else {
            self.flush_system_time(output);
        }
    }

    /// Final flush invoked when the input stream closes. In event-time mode
    /// this drains every remaining bucket regardless of the wall-clock
    /// predicate so that metrics in still-open windows are emitted on
    /// shutdown or topology reload, matching system-time semantics where
    /// `flush_system_time` always empties `self.map`.
    pub(crate) fn flush_final(&mut self, output: &mut Vec<Event>) {
        if self.config.is_event_time() {
            self.flush_event_time_buckets(output, true);
        } else {
            self.flush_system_time(output);
        }
    }

    fn flush_system_time(&mut self, output: &mut Vec<Event>) {
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
                                // Drain any remaining event-time buckets on
                                // shutdown so in-flight metrics still flow
                                // downstream.
                                self.flush_final(&mut output);
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
