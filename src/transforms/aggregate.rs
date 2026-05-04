use std::{
    collections::{BTreeMap, HashMap, hash_map::Entry},
    pin::Pin,
    time::Duration,
};

use async_stream::stream;
use chrono::{DateTime, Utc};
use futures::{Stream, StreamExt};
use vector_lib::{
    configurable::configurable_component,
    event::{
        MetricValue,
        metric::{Metric, MetricData, MetricKind, MetricSeries},
    },
};

use crate::{
    config::{DataType, Input, OutputId, TransformConfig, TransformContext, TransformOutput},
    event::{Event, EventMetadata},
    internal_events::{
        AggregateEventDropped, AggregateEventRecorded, AggregateFlushed, AggregateUpdateFailed,
    },
    schema,
    transforms::{TaskTransform, Transform},
};

/// Configuration for the `aggregate` transform.
#[configurable_component(transform("aggregate", "Aggregate metrics passing through a topology."))]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
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
    /// Time source to use for aggregation windows.
    ///
    /// When set to `event_time`, events are grouped into buckets based on their timestamps rather than
    /// when they are processed. Events arriving out of order (after their bucket has been flushed) are rejected.
    #[serde(default = "default_time_source")]
    #[configurable(derived)]
    pub time_source: TimeSource,

    /// Grace period for late-arriving events when using event-time aggregation.
    ///
    /// Each bucket is held open for this many milliseconds past the end of its window so late
    /// events still land in the correct bucket. Once a bucket is emitted it is closed
    /// permanently; any later events whose timestamp falls inside it are dropped and counted
    /// via `component_discarded_events_total`.
    ///
    /// Set to 0 for strict ordering (no late events allowed). Only applies when `time_source`
    /// is set to `event_time`.
    #[serde(default)]
    #[configurable(metadata(docs::examples = 0, docs::examples = 5000, docs::examples = 30000))]
    pub allowed_lateness_ms: u64,

    /// How to handle events with missing timestamps in event-time mode.
    ///
    /// When `true`, events without a timestamp use the current system time as a fallback.
    /// When `false`, such events are dropped and counted via `component_discarded_events_total`.
    ///
    /// Only applies when `time_source` is set to `event_time`.
    #[serde(default)]
    pub use_system_time_for_missing_timestamps: bool,

    /// Maximum allowed time drift for future events in event-time mode.
    ///
    /// Acts as a clock-skew guard: events whose timestamp is further in the future than this
    /// many milliseconds (relative to the current system time) are dropped and counted via
    /// `component_discarded_events_total`. Defaults to 10 seconds.
    ///
    /// Set to 0 to allow events at any future time. Only applies when `time_source` is set
    /// to `event_time`.
    #[serde(default = "default_max_future_ms")]
    #[configurable(metadata(docs::examples = 0, docs::examples = 60000, docs::examples = 300000))]
    pub max_future_ms: u64,
}

#[configurable_component]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[configurable(description = "The time source to use for aggregation windows.")]
pub enum TimeSource {
    /// Use system clock time for aggregation windows (default).
    ///
    /// Events are aggregated based on when they are processed, not their timestamps.
    #[default]
    SystemTime,

    /// Use event timestamps for aggregation windows.
    ///
    /// Events are grouped into buckets based on their timestamps. Events arriving out of order
    /// (after their bucket has been flushed) are rejected.
    EventTime,
}

const fn default_time_source() -> TimeSource {
    TimeSource::SystemTime
}

const fn default_max_future_ms() -> u64 {
    10000 // 10 seconds default
}

#[configurable_component]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
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

#[derive(Clone, Debug, Default, PartialEq)]
enum InnerMode {
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
    Diff {
        prev_map: HashMap<MetricSeries, MetricEntry>,
    },

    /// Max value of absolute metric, ignores incremental
    Max,

    /// Min value of absolute metric, ignores incremental
    Min,

    /// Mean value of absolute metric, ignores incremental
    Mean {
        multi_map: HashMap<MetricSeries, Vec<MetricEntry>>,
    },

    /// Stdev value of absolute metric, ignores incremental
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
        _: &TransformContext,
        _: &[(OutputId, schema::Definition)],
    ) -> Vec<TransformOutput> {
        vec![TransformOutput::new(DataType::Metric, HashMap::new())]
    }
}

type MetricEntry = (MetricData, EventMetadata);

type BucketKey = i64;

#[derive(Debug)]
pub struct Aggregate {
    interval: Duration,
    map: HashMap<MetricSeries, MetricEntry>,
    mode: InnerMode,
    time_source: TimeSource,
    event_time_buckets: BTreeMap<BucketKey, HashMap<MetricSeries, MetricEntry>>,
    event_time_prev_buckets: BTreeMap<BucketKey, HashMap<MetricSeries, MetricEntry>>,
    event_time_multi_buckets: BTreeMap<BucketKey, HashMap<MetricSeries, Vec<MetricEntry>>>,
    watermark: Option<BucketKey>,
    config: AggregateConfig,
}

impl Aggregate {
    pub fn new(config: &AggregateConfig) -> crate::Result<Self> {
        Ok(Self {
            interval: Duration::from_millis(config.interval_ms),
            map: Default::default(),
            mode: config.mode.into(),
            time_source: config.time_source,
            event_time_buckets: Default::default(),
            event_time_prev_buckets: Default::default(),
            event_time_multi_buckets: Default::default(),
            watermark: None,
            config: *config,
        })
    }

    /// Start of the half-open window `[bucket_key, bucket_key + interval_ms)` containing
    /// `timestamp`, aligned to multiples of `interval_ms` from the Unix epoch.
    ///
    /// Euclidean division (`div_euclid`) is required: Rust's truncating `/`
    /// rounds toward zero, so timestamps just before the epoch (negative
    /// millis) would incorrectly map into the non-negative bucket `[0, interval)`
    /// instead of `[-interval, 0)`.
    const fn bucket_key(&self, timestamp: DateTime<Utc>) -> BucketKey {
        let timestamp_ms = timestamp.timestamp_millis();
        let interval_ms = self.interval.as_millis() as i64;
        timestamp_ms.div_euclid(interval_ms).saturating_mul(interval_ms)
    }

    /// Returns `true` if `bucket_key` belongs to a window that has already
    /// been emitted and therefore must not accept any further events.
    ///
    /// `watermark` is the *exclusive end* of the highest bucket flushed so
    /// far -- equivalently, the smallest `bucket_key` that is still valid to
    /// record into. `allowed_lateness_ms` is honoured at flush time (it
    /// delays closing the bucket); once a window has been emitted it is
    /// closed unconditionally and late events for it are dropped.
    const fn is_too_late(&self, bucket_key: BucketKey) -> bool {
        if let Some(watermark) = self.watermark {
            bucket_key < watermark
        } else {
            false
        }
    }

    pub fn record(&mut self, event: Event) {
        let metric = event.into_metric();
        let timestamp = metric.timestamp();
        let (series, mut data, metadata) = metric.into_parts();

        if self.time_source == TimeSource::EventTime {
            // Handle missing timestamp
            let ts = match timestamp {
                Some(ts) => ts,
                None => {
                    if self.config.use_system_time_for_missing_timestamps {
                        Utc::now()
                    } else {
                        emit!(AggregateEventDropped {
                            reason: "Event missing timestamp required for event-time aggregation."
                        });
                        return;
                    }
                }
            };
            // Preserve (or synthesize) the timestamp in the stored metric so that
            // event-time "latest" selection can compare timestamps reliably.
            data.time.timestamp = Some(ts);

            // Check for future timestamps
            if self.config.max_future_ms > 0 {
                let now = Utc::now();
                let max_future =
                    now + chrono::Duration::milliseconds(self.config.max_future_ms as i64);
                if ts > max_future {
                    emit!(AggregateEventDropped {
                        reason: "Event timestamp too far in the future."
                    });
                    return;
                }
            }

            let bucket_key = self.bucket_key(ts);

            // Check if event is too late (past watermark with grace period)
            if self.is_too_late(bucket_key) {
                emit!(AggregateEventDropped {
                    reason: "Event timestamp is too late; bucket already flushed."
                });
                return;
            }

            // `record_into_bucket` emits its own `AggregateEventDropped`
            // telemetry when the event is incompatible with the configured
            // mode; only count it as recorded when it actually landed.
            if self.record_into_bucket(bucket_key, series, data, metadata) {
                emit!(AggregateEventRecorded);
            }
            return;
        }
        match self.config.mode {
            AggregationMode::Auto => match data.kind {
                MetricKind::Incremental => Self::record_sum(&mut self.map, series, data, metadata),
                MetricKind::Absolute => {
                    self.map.insert(series, (data, metadata));
                }
            },
            AggregationMode::Sum => Self::record_sum(&mut self.map, series, data, metadata),
            AggregationMode::Latest | AggregationMode::Diff => match data.kind {
                MetricKind::Incremental => (),
                MetricKind::Absolute => {
                    self.map.insert(series, (data, metadata));
                }
            },
            AggregationMode::Count => Self::record_count(&mut self.map, series, data, metadata),
            AggregationMode::Max | AggregationMode::Min => {
                Self::record_comparison(&mut self.map, series, data, metadata, self.config.mode)
            }
            AggregationMode::Mean | AggregationMode::Stdev => {
                if let InnerMode::Mean { multi_map } | InnerMode::Stdev { multi_map } =
                    &mut self.mode
                {
                    match data.kind {
                        MetricKind::Incremental => (),
                        MetricKind::Absolute => {
                            if matches!(data.value, MetricValue::Gauge { value: _ }) {
                                match multi_map.entry(series) {
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
                    }
                }
            }
        }

        emit!(AggregateEventRecorded);
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
    const fn will_be_stored(mode: AggregationMode, data: &MetricData) -> bool {
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

    /// Records an event-time event into the appropriate bucket. Returns `true`
    /// if the event was stored, `false` if it was dropped because its
    /// (kind, value) is incompatible with the configured mode (for example
    /// an `Incremental` event arriving at a `Mean`-configured aggregator).
    ///
    /// Bucket entries are created lazily inside each per-mode arm so that
    /// dropped events do not allocate or advance the watermark on flush.
    /// The `multi_bucket` map is only touched in `Mean`/`Stdev` modes.
    fn record_into_bucket(
        &mut self,
        bucket_key: BucketKey,
        series: MetricSeries,
        data: MetricData,
        metadata: EventMetadata,
    ) -> bool {
        let mode = self.config.mode;

        // Drop incompatible events explicitly (and emit telemetry for them)
        // before touching either bucket map -- see `will_be_stored`.
        if !Self::will_be_stored(mode, &data) {
            emit!(AggregateEventDropped {
                reason: "Event kind/value is incompatible with the configured aggregation mode."
            });
            return false;
        }

        match mode {
            AggregationMode::Auto => {
                let bucket = self.event_time_buckets.entry(bucket_key).or_default();
                match data.kind {
                    MetricKind::Incremental => {
                        Self::record_sum(bucket, series, data, metadata);
                    }
                    MetricKind::Absolute => match bucket.entry(series) {
                        Entry::Vacant(entry) => {
                            entry.insert((data, metadata));
                        }
                        Entry::Occupied(mut entry) => {
                            // In event-time mode, "latest" means latest *event timestamp*
                            // within the time bucket, not latest arrival order.
                            let new_ts = data.timestamp().cloned();
                            let existing_ts = entry.get().0.timestamp().cloned();
                            let should_replace = match (new_ts, existing_ts) {
                                (Some(n), Some(e)) => n >= e,
                                (Some(_), None) => true,
                                _ => false,
                            };
                            if should_replace {
                                *entry.get_mut() = (data, metadata);
                            }
                        }
                    },
                }
            }
            AggregationMode::Sum => {
                let bucket = self.event_time_buckets.entry(bucket_key).or_default();
                Self::record_sum(bucket, series, data, metadata);
            }
            AggregationMode::Latest | AggregationMode::Diff => {
                // `data.kind == Absolute` is guaranteed by `will_be_stored`.
                let bucket = self.event_time_buckets.entry(bucket_key).or_default();
                match bucket.entry(series) {
                    Entry::Vacant(entry) => {
                        entry.insert((data, metadata));
                    }
                    Entry::Occupied(mut entry) => {
                        let new_ts = data.timestamp().cloned();
                        let existing_ts = entry.get().0.timestamp().cloned();
                        let should_replace = match (new_ts, existing_ts) {
                            (Some(n), Some(e)) => n >= e,
                            (Some(_), None) => true,
                            _ => false,
                        };
                        if should_replace {
                            *entry.get_mut() = (data, metadata);
                        }
                    }
                }
            }
            AggregationMode::Count => {
                let bucket = self.event_time_buckets.entry(bucket_key).or_default();
                Self::record_count(bucket, series, data, metadata);
            }
            AggregationMode::Max | AggregationMode::Min => {
                let bucket = self.event_time_buckets.entry(bucket_key).or_default();
                Self::record_comparison(bucket, series, data, metadata, mode);
            }
            AggregationMode::Mean | AggregationMode::Stdev => {
                // `will_be_stored` has already guaranteed Absolute + Gauge.
                //
                // Mean/Stdev write samples into `event_time_multi_buckets`;
                // an empty entry is also placed in `event_time_buckets` so
                // the flush loop -- which iterates `event_time_buckets.keys()`
                // to discover eligible buckets -- picks this bucket up too.
                self.event_time_buckets.entry(bucket_key).or_default();
                let multi_bucket = self.event_time_multi_buckets.entry(bucket_key).or_default();
                match multi_bucket.entry(series) {
                    Entry::Occupied(mut entry) => {
                        entry.get_mut().push((data, metadata));
                    }
                    Entry::Vacant(entry) => {
                        entry.insert(vec![(data, metadata)]);
                    }
                }
            }
        }

        true
    }

    fn record_sum(
        bucket: &mut HashMap<MetricSeries, MetricEntry>,
        series: MetricSeries,
        data: MetricData,
        metadata: EventMetadata,
    ) {
        match data.kind {
            MetricKind::Incremental => match bucket.entry(series) {
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

    fn record_count(
        bucket: &mut HashMap<MetricSeries, MetricEntry>,
        series: MetricSeries,
        mut data: MetricData,
        metadata: EventMetadata,
    ) {
        let mut count_data = data.clone();
        let existing = bucket.entry(series).or_insert_with(|| {
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

    fn record_comparison(
        bucket: &mut HashMap<MetricSeries, MetricEntry>,
        series: MetricSeries,
        data: MetricData,
        metadata: EventMetadata,
        mode: AggregationMode,
    ) {
        match data.kind {
            MetricKind::Incremental => (),
            MetricKind::Absolute => match bucket.entry(series) {
                Entry::Occupied(mut entry) => {
                    let existing = entry.get_mut();
                    // In order to update (add) the new and old kind's must match
                    if existing.0.kind == data.kind {
                        if let MetricValue::Gauge {
                            value: existing_value,
                        } = existing.0.value()
                            && let MetricValue::Gauge { value: new_value } = data.value()
                        {
                            let should_update = match mode {
                                AggregationMode::Max => new_value > existing_value,
                                AggregationMode::Min => new_value < existing_value,
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
            },
        }
    }

    pub fn flush_into(&mut self, output: &mut Vec<Event>) {
        if self.time_source == TimeSource::EventTime {
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
    fn flush_final(&mut self, output: &mut Vec<Event>) {
        if self.time_source == TimeSource::EventTime {
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

    fn flush_event_time_buckets(&mut self, output: &mut Vec<Event>, force: bool) {
        let now = Utc::now();
        let now_ms = now.timestamp_millis();
        let interval_ms = self.interval.as_millis() as i64;
        let grace_ms = self.config.allowed_lateness_ms as i64;

        // A bucket [bucket_key, bucket_key + interval) is eligible to flush
        // when either:
        //   1. `now >= bucket_key + interval + allowed_lateness` -- the
        //      bucket has ended and its grace period has elapsed; or
        //   2. `force` is true -- final flush on shutdown / topology reload,
        //      which drains every remaining bucket so in-flight events are
        //      not silently dropped.
        //
        // The watermark advanced below gates late-event rejection via
        // `is_too_late()`.
        let buckets_to_flush: Vec<BucketKey> = self
            .event_time_buckets
            .keys()
            .filter(|&&bucket_key| force || now_ms >= bucket_key + interval_ms + grace_ms)
            .copied()
            .collect();

        for bucket_key in buckets_to_flush {
            if let Some(bucket_map) = self.event_time_buckets.remove(&bucket_key) {
                for (series, entry) in bucket_map.clone().into_iter() {
                    let mut metric = Metric::from_parts(series, entry.0, entry.1);
                    let prev_bucket_key = bucket_key.saturating_sub(interval_ms);
                    if matches!(self.config.mode, AggregationMode::Diff)
                        && let Some(prev_bucket) =
                            self.event_time_prev_buckets.get(&prev_bucket_key)
                        && let Some(prev_entry) = prev_bucket.get(metric.series())
                        && metric.data().kind == prev_entry.0.kind
                        && !metric.subtract(&prev_entry.0)
                    {
                        emit!(AggregateUpdateFailed);
                    }
                    output.push(Event::Metric(metric));
                }

                if let Some(multi_bucket) = self.event_time_multi_buckets.remove(&bucket_key) {
                    'outer: for (series, entries) in multi_bucket.into_iter() {
                        if entries.is_empty() {
                            continue;
                        }

                        let (mut final_sum, mut final_metadata) = entries.first().unwrap().clone();
                        for (data, metadata) in entries.iter().skip(1) {
                            if !final_sum.update(data) {
                                emit!(AggregateUpdateFailed);
                                continue 'outer;
                            }
                            final_metadata.merge(metadata.clone());
                        }

                        let final_mean_value =
                            if let MetricValue::Gauge { value } = final_sum.value_mut() {
                                *value /= entries.len() as f64;
                                *value
                            } else {
                                0.0
                            };

                        let final_mean = final_sum.clone();
                        match self.config.mode {
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
                                let metric =
                                    Metric::from_parts(series, final_stdev, final_metadata);
                                output.push(Event::Metric(metric));
                            }
                            _ => (),
                        }
                    }
                }

                // `event_time_prev_buckets` is read only by `Diff` mode (see the
                // flush branch above), so it is also populated only in `Diff`
                // mode. Other modes never need a previous bucket and would
                // accumulate entries unboundedly if it were populated here.
                if matches!(self.config.mode, AggregationMode::Diff) {
                    self.event_time_prev_buckets.insert(bucket_key, bucket_map);
                    // Keep only a small rolling window for diffing against the
                    // immediately preceding bucket.
                    let min_keep = bucket_key.saturating_sub(interval_ms);
                    self.event_time_prev_buckets.retain(|&k, _| k >= min_keep);
                }
            }

            // Advance the watermark to the *exclusive end* of the highest
            // flushed bucket so subsequent events for that window (or any
            // earlier one) are rejected by `is_too_late`.
            let bucket_end = bucket_key.saturating_add(interval_ms);
            if self.watermark.is_none_or(|w| bucket_end > w) {
                self.watermark = Some(bucket_end);
            }
        }

        if !output.is_empty() {
            emit!(AggregateFlushed);
        }
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

    use chrono::TimeZone;
    use futures::stream;
    use tokio::sync::mpsc;
    use tokio_stream::wrappers::ReceiverStream;
    use vector_lib::config::{ComponentKey, LogNamespace};
    use vrl::value::Kind;

    use super::*;
    use crate::{
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
        event.metadata_mut().set_schema_definition(&Arc::new(
            Definition::new_with_default_metadata(Kind::any_object(), [LogNamespace::Legacy]),
        ));

        event.metadata_mut().set_source_type("unit_test_stream");

        event
    }

    fn make_metric_with_timestamp(
        name: &'static str,
        kind: MetricKind,
        value: MetricValue,
        timestamp: DateTime<Utc>,
    ) -> Event {
        let mut event =
            Event::Metric(Metric::new(name, kind, value).with_timestamp(Some(timestamp)))
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
            time_source: TimeSource::SystemTime,
            allowed_lateness_ms: 0,
            use_system_time_for_missing_timestamps: false,
            max_future_ms: 10000,
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
            time_source: TimeSource::SystemTime,
            allowed_lateness_ms: 0,
            use_system_time_for_missing_timestamps: false,
            max_future_ms: 10000,
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
            time_source: TimeSource::SystemTime,
            allowed_lateness_ms: 0,
            use_system_time_for_missing_timestamps: false,
            max_future_ms: 10000,
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
            time_source: TimeSource::SystemTime,
            allowed_lateness_ms: 0,
            use_system_time_for_missing_timestamps: false,
            max_future_ms: 10000,
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
            time_source: TimeSource::SystemTime,
            allowed_lateness_ms: 0,
            use_system_time_for_missing_timestamps: false,
            max_future_ms: 10000,
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
            time_source: TimeSource::SystemTime,
            allowed_lateness_ms: 0,
            use_system_time_for_missing_timestamps: false,
            max_future_ms: 10000,
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
            time_source: TimeSource::SystemTime,
            allowed_lateness_ms: 0,
            use_system_time_for_missing_timestamps: false,
            max_future_ms: 10000,
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
            time_source: TimeSource::SystemTime,
            allowed_lateness_ms: 0,
            use_system_time_for_missing_timestamps: false,
            max_future_ms: 10000,
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
            time_source: TimeSource::SystemTime,
            allowed_lateness_ms: 0,
            use_system_time_for_missing_timestamps: false,
            max_future_ms: 10000,
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
            time_source: TimeSource::SystemTime,
            allowed_lateness_ms: 0,
            use_system_time_for_missing_timestamps: false,
            max_future_ms: 10000,
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
            time_source: TimeSource::SystemTime,
            allowed_lateness_ms: 0,
            use_system_time_for_missing_timestamps: false,
            max_future_ms: 10000,
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
            r"
interval_ms = 999999
",
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

    #[test]
    fn event_time_incremental_auto() {
        let mut agg = Aggregate::new(&AggregateConfig {
            interval_ms: 10000_u64, // 10 seconds
            mode: AggregationMode::Auto,
            time_source: TimeSource::EventTime,
            allowed_lateness_ms: 0,
            use_system_time_for_missing_timestamps: false,
            max_future_ms: 10000,
        })
        .unwrap();

        // Create events with timestamps in the same bucket (11:00:20 to 11:00:30)
        let base_time = DateTime::parse_from_rfc3339("2025-12-29T11:00:20Z")
            .unwrap()
            .with_timezone(&Utc);

        let counter_a_1 = make_metric_with_timestamp(
            "counter_a",
            MetricKind::Incremental,
            MetricValue::Counter { value: 42.0 },
            base_time,
        );
        let counter_a_2 = make_metric_with_timestamp(
            "counter_a",
            MetricKind::Incremental,
            MetricValue::Counter { value: 43.0 },
            base_time + chrono::Duration::seconds(5), // 11:00:25
        );
        // Record events in the same bucket
        agg.record(counter_a_1.clone());
        agg.record(counter_a_2);
        let mut out = vec![];
        // Flush should aggregate them together
        agg.flush_into(&mut out);
        assert_eq!(1, out.len());
        // Check that values are summed (we can't easily compare full events, so check the value)
        let metric = out[0].as_metric();
        assert_eq!(metric.series().name.name.as_str(), "counter_a");
        if let MetricValue::Counter { value } = metric.value() {
            assert_eq!(*value, 85.0);
        } else {
            panic!("Expected Counter value");
        }
    }

    /// Rust truncating `/` rounds toward zero, so `-1 / 10000 == 0` and the
    /// bucket anchor would wrongly be `0`. Euclidean alignment places
    /// `-1ms` in `[-interval_ms, 0)` anchored at `-interval_ms`.
    #[test]
    fn event_time_pre_epoch_buckets_use_floor_division() {
        let mut agg = Aggregate::new(&AggregateConfig {
            interval_ms: 10_000_u64,
            mode: AggregationMode::Auto,
            time_source: TimeSource::EventTime,
            allowed_lateness_ms: 0,
            use_system_time_for_missing_timestamps: false,
            max_future_ms: 10_000,
        })
        .unwrap();

        let ts = Utc
            .timestamp_millis_opt(-1)
            .latest()
            .expect("valid millis near epoch");

        agg.record(make_metric_with_timestamp(
            "pre_epoch",
            MetricKind::Incremental,
            MetricValue::Counter { value: 1.0 },
            ts,
        ));

        assert_eq!(
            agg.event_time_buckets.keys().next().copied(),
            Some(-10_000),
            "-1 ms must bucket to [-10000, 0), not [0, 10000)"
        );
    }

    #[test]
    fn event_time_out_of_order_rejection() {
        let mut agg = Aggregate::new(&AggregateConfig {
            interval_ms: 10000_u64, // 10 seconds
            mode: AggregationMode::Auto,
            time_source: TimeSource::EventTime,
            allowed_lateness_ms: 0,
            use_system_time_for_missing_timestamps: false,
            max_future_ms: 10000,
        })
        .unwrap();

        let base_time = DateTime::parse_from_rfc3339("2025-12-29T11:00:20Z")
            .unwrap()
            .with_timezone(&Utc);

        // First event at 11:00:30 (end of first bucket)
        let event1 = make_metric_with_timestamp(
            "counter_a",
            MetricKind::Incremental,
            MetricValue::Counter { value: 42.0 },
            base_time + chrono::Duration::seconds(10), // 11:00:30
        );

        // Second event at 11:00:50 (second bucket)
        let event2 = make_metric_with_timestamp(
            "counter_a",
            MetricKind::Incremental,
            MetricValue::Counter { value: 43.0 },
            base_time + chrono::Duration::seconds(30), // 11:00:50
        );

        // Record first event and flush (this will set watermark)
        agg.record(event1);
        let mut out = vec![];
        agg.flush_into(&mut out);
        assert_eq!(1, out.len());

        // Now record an out-of-order event from the first bucket (11:00:25)
        let late_event = make_metric_with_timestamp(
            "counter_a",
            MetricKind::Incremental,
            MetricValue::Counter { value: 100.0 },
            base_time + chrono::Duration::seconds(5), // 11:00:25 - should be rejected
        );

        // This should be rejected (too late)
        agg.record(late_event);
        out.clear();
        agg.flush_into(&mut out);
        // Should have no new output (late event was rejected)
        assert_eq!(0, out.len());

        // Record event from second bucket (should be accepted)
        agg.record(event2);
        out.clear();
        agg.flush_into(&mut out);
        assert_eq!(1, out.len());
    }

    #[test]
    fn event_time_different_buckets() {
        let mut agg = Aggregate::new(&AggregateConfig {
            interval_ms: 10000_u64, // 10 seconds
            mode: AggregationMode::Auto,
            time_source: TimeSource::EventTime,
            allowed_lateness_ms: 0,
            use_system_time_for_missing_timestamps: false,
            max_future_ms: 10000,
        })
        .unwrap();

        let base_time = DateTime::parse_from_rfc3339("2025-12-29T11:00:20Z")
            .unwrap()
            .with_timezone(&Utc);

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
            base_time + chrono::Duration::seconds(5), // 11:00:25
        );

        // Events in second bucket (11:00:30 - 11:00:40)
        let event1_bucket2 = make_metric_with_timestamp(
            "counter_a",
            MetricKind::Incremental,
            MetricValue::Counter { value: 30.0 },
            base_time + chrono::Duration::seconds(15), // 11:00:35
        );

        // Record events from first bucket
        agg.record(event1_bucket1);
        agg.record(event2_bucket1);
        let mut out = vec![];
        agg.flush_into(&mut out);
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
        agg.flush_into(&mut out);
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
    fn event_time_no_timestamp_rejected() {
        let mut agg = Aggregate::new(&AggregateConfig {
            interval_ms: 10000_u64,
            mode: AggregationMode::Auto,
            time_source: TimeSource::EventTime,
            allowed_lateness_ms: 0,
            use_system_time_for_missing_timestamps: false,
            max_future_ms: 10000,
        })
        .unwrap();

        // Event without timestamp should be rejected
        let event_no_ts = make_metric(
            "counter_a",
            MetricKind::Incremental,
            MetricValue::Counter { value: 42.0 },
        );

        agg.record(event_no_ts);
        let mut out = vec![];
        agg.flush_into(&mut out);
        // Should have no output (event was rejected)
        assert_eq!(0, out.len());
    }

    #[test]
    fn event_time_absolute_latest() {
        let mut agg = Aggregate::new(&AggregateConfig {
            interval_ms: 10000_u64,
            mode: AggregationMode::Auto,
            time_source: TimeSource::EventTime,
            allowed_lateness_ms: 0,
            use_system_time_for_missing_timestamps: false,
            max_future_ms: 10000,
        })
        .unwrap();

        let base_time = DateTime::parse_from_rfc3339("2025-12-29T11:00:20Z")
            .unwrap()
            .with_timezone(&Utc);

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
            base_time + chrono::Duration::seconds(5),
        );

        // Record out of timestamp order: latest means latest event timestamp, not arrival order.
        agg.record(gauge2);
        agg.record(gauge1);
        let mut out = vec![];
        agg.flush_into(&mut out);
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
    fn event_time_mean_happy_path() {
        let mut agg = Aggregate::new(&AggregateConfig {
            interval_ms: 10000_u64,
            mode: AggregationMode::Mean,
            time_source: TimeSource::EventTime,
            allowed_lateness_ms: 0,
            use_system_time_for_missing_timestamps: false,
            max_future_ms: 10000,
        })
        .unwrap();

        let base_time = DateTime::parse_from_rfc3339("2025-12-29T11:00:20Z")
            .unwrap()
            .with_timezone(&Utc);

        // Three absolute gauges for the same series in one event-time bucket.
        agg.record(make_metric_with_timestamp(
            "gauge_mean_et",
            MetricKind::Absolute,
            MetricValue::Gauge { value: 32.0 },
            base_time + chrono::Duration::seconds(1),
        ));
        agg.record(make_metric_with_timestamp(
            "gauge_mean_et",
            MetricKind::Absolute,
            MetricValue::Gauge { value: 82.0 },
            base_time + chrono::Duration::seconds(2),
        ));
        agg.record(make_metric_with_timestamp(
            "gauge_mean_et",
            MetricKind::Absolute,
            MetricValue::Gauge { value: 51.0 },
            base_time + chrono::Duration::seconds(3),
        ));

        let mut out = vec![];
        agg.flush_into(&mut out);
        assert_eq!(1, out.len());
        assert_eq!(
            out[0].as_metric().series().name.name.as_str(),
            "gauge_mean_et"
        );
        if let MetricValue::Gauge { value } = out[0].as_metric().value() {
            assert!(
                (*value - 55.0).abs() < 1e-9,
                "expected mean 55.0, got {value}"
            );
        } else {
            panic!("Expected Gauge metric value");
        }
    }

    #[test]
    fn event_time_stdev_happy_path() {
        let mut agg = Aggregate::new(&AggregateConfig {
            interval_ms: 10000_u64,
            mode: AggregationMode::Stdev,
            time_source: TimeSource::EventTime,
            allowed_lateness_ms: 0,
            use_system_time_for_missing_timestamps: false,
            max_future_ms: 10000,
        })
        .unwrap();

        let base_time = DateTime::parse_from_rfc3339("2025-12-29T11:00:25Z")
            .unwrap()
            .with_timezone(&Utc);

        let values = [25.0f64, 30.0, 35.0, 40.0, 45.0, 50.0, 55.0];
        for (i, &v) in values.iter().enumerate() {
            agg.record(make_metric_with_timestamp(
                "gauge_stdev_et",
                MetricKind::Absolute,
                MetricValue::Gauge { value: v },
                base_time + chrono::Duration::milliseconds(100 * i as i64),
            ));
        }

        let mut out = vec![];
        agg.flush_into(&mut out);
        assert_eq!(1, out.len());
        assert_eq!(
            out[0].as_metric().series().name.name.as_str(),
            "gauge_stdev_et"
        );
        if let MetricValue::Gauge { value } = out[0].as_metric().value() {
            assert!(
                (*value - 10.0).abs() < 1e-9,
                "expected stdev 10.0, got {value}"
            );
        } else {
            panic!("Expected Gauge metric value");
        }
    }

    #[test]
    fn event_time_diff_uses_previous_bucket() {
        let mut agg = Aggregate::new(&AggregateConfig {
            interval_ms: 10000_u64, // 10 seconds
            mode: AggregationMode::Diff,
            time_source: TimeSource::EventTime,
            allowed_lateness_ms: 0,
            use_system_time_for_missing_timestamps: false,
            max_future_ms: 10000,
        })
        .unwrap();

        let base_time = DateTime::parse_from_rfc3339("2025-12-29T11:00:20Z")
            .unwrap()
            .with_timezone(&Utc);

        let interval_i64 = 10000_i64;
        let bucket1_key = (base_time.timestamp_millis() / interval_i64) * interval_i64;
        let bucket2_key = bucket1_key + interval_i64;

        let ts1 = Utc
            .timestamp_millis_opt(bucket1_key + 5_000)
            .latest()
            .unwrap();
        let ts2 = Utc
            .timestamp_millis_opt(bucket2_key + 5_000)
            .latest()
            .unwrap();

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
        agg.flush_into(&mut out);

        // Both buckets are far in the past, so both flush immediately in this unit test.
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

    #[test]
    fn event_time_allowed_lateness_delays_flush_for_current_bucket() {
        let interval_ms = 5000_u64;
        let allowed_lateness_ms = 3000_u64;

        let mut agg = Aggregate::new(&AggregateConfig {
            interval_ms,
            mode: AggregationMode::Auto,
            time_source: TimeSource::EventTime,
            allowed_lateness_ms,
            use_system_time_for_missing_timestamps: false,
            max_future_ms: 10000,
        })
        .unwrap();

        // Put an event in the *current* bucket. With allowed_lateness configured, the bucket should
        // not be flushable until it has completed + grace window.
        let now = Utc::now();
        let now_ms = now.timestamp_millis();
        let interval_i64 = interval_ms as i64;
        let current_bucket_key = (now_ms / interval_i64) * interval_i64;

        let ts = Utc
            .timestamp_millis_opt(current_bucket_key + interval_i64 - 1000)
            .latest()
            .unwrap();

        let gauge = make_metric_with_timestamp(
            "gauge_lateness",
            MetricKind::Absolute,
            MetricValue::Gauge { value: 42.0 },
            ts,
        );
        agg.record(gauge);

        let mut out = vec![];
        agg.flush_into(&mut out);
        assert_eq!(0, out.len());
    }

    #[test]
    fn event_time_closed_buckets_are_rejected_regardless_of_grace() {
        // `allowed_lateness_ms` controls only when a bucket is *closed* -- it delays
        // the initial flush so late-arriving events still land in the correct bucket.
        // Once a bucket has been emitted, however, a late event for that bucket
        // (or any earlier bucket) must be rejected: accepting it would re-create the
        // bucket and emit a duplicate partial aggregate for an already-closed window.
        let interval_ms = 10_000_u64;
        let allowed_lateness_ms = 10_000_u64;

        let mut agg = Aggregate::new(&AggregateConfig {
            interval_ms,
            mode: AggregationMode::Auto,
            time_source: TimeSource::EventTime,
            allowed_lateness_ms,
            use_system_time_for_missing_timestamps: false,
            max_future_ms: 10000,
        })
        .unwrap();

        let now_ms = Utc::now().timestamp_millis();
        let interval_i64 = interval_ms as i64;

        let current_bucket_key = (now_ms / interval_i64) * interval_i64;
        // Far enough in the past that the wall-clock + grace flush predicate is
        // satisfied for the initial bucket.
        let watermark_bucket_key = current_bucket_key - interval_i64 * 5;
        // Same bucket that just got flushed.
        let same_bucket_late_key = watermark_bucket_key;
        // Earlier bucket within the grace window (would have been accepted under the
        // pre-fix semantics, must now be rejected).
        let earlier_within_grace_key = watermark_bucket_key - interval_i64;
        // The *next* (future) open bucket -- still admissible.
        let next_open_bucket_key = watermark_bucket_key + interval_i64;

        let ts = |k: i64| {
            Utc.timestamp_millis_opt(k + interval_i64 / 2)
                .latest()
                .unwrap()
        };

        // Flush the initial bucket so the watermark advances past it.
        agg.record(make_metric_with_timestamp(
            "gauge_closed_bucket_rejection",
            MetricKind::Absolute,
            MetricValue::Gauge { value: 1.0 },
            ts(watermark_bucket_key),
        ));
        let mut out = vec![];
        agg.flush_into(&mut out);
        assert_eq!(1, out.len());
        assert_eq!(
            agg.watermark,
            Some(watermark_bucket_key + interval_i64),
            "watermark must point to the END (exclusive) of the flushed bucket"
        );
        out.clear();

        // Late events for the just-flushed bucket and any earlier bucket within
        // the grace window must be rejected.
        agg.record(make_metric_with_timestamp(
            "gauge_closed_bucket_rejection",
            MetricKind::Absolute,
            MetricValue::Gauge { value: 99.0 },
            ts(same_bucket_late_key),
        ));
        agg.record(make_metric_with_timestamp(
            "gauge_closed_bucket_rejection",
            MetricKind::Absolute,
            MetricValue::Gauge { value: 99.0 },
            ts(earlier_within_grace_key),
        ));
        agg.flush_into(&mut out);
        assert!(
            out.is_empty(),
            "events for already-closed buckets must not produce a duplicate aggregate"
        );

        // An event for the next open bucket must still be accepted.
        agg.record(make_metric_with_timestamp(
            "gauge_closed_bucket_rejection",
            MetricKind::Absolute,
            MetricValue::Gauge { value: 7.0 },
            ts(next_open_bucket_key),
        ));
        agg.flush_into(&mut out);
        assert_eq!(1, out.len(), "next open bucket must still flush");
        if let MetricValue::Gauge { value } = out[0].as_metric().value() {
            assert_eq!(*value, 7.0);
        } else {
            panic!("Expected Gauge metric value");
        }
    }

    /// In any non-`Diff` mode the `event_time_prev_buckets` map is never
    /// consulted, so it must also never be populated -- otherwise long-running
    /// aggregators would grow proportional to
    /// (unique series in interval) × (intervals since startup).
    #[test]
    fn event_time_non_diff_modes_do_not_retain_prev_buckets() {
        let interval_ms = 10_000_u64;

        let mut agg = Aggregate::new(&AggregateConfig {
            interval_ms,
            mode: AggregationMode::Auto,
            time_source: TimeSource::EventTime,
            allowed_lateness_ms: 0,
            use_system_time_for_missing_timestamps: false,
            max_future_ms: 10000,
        })
        .unwrap();

        let now_ms = Utc::now().timestamp_millis();
        let interval_i64 = interval_ms as i64;
        // Use buckets far in the past so each one flushes on the first call.
        let mut bucket_key = (now_ms / interval_i64) * interval_i64 - interval_i64 * 100;

        for _ in 0..50 {
            let ts = Utc
                .timestamp_millis_opt(bucket_key + interval_i64 / 2)
                .latest()
                .unwrap();
            agg.record(make_metric_with_timestamp(
                "leak_probe",
                MetricKind::Incremental,
                MetricValue::Counter { value: 1.0 },
                ts,
            ));
            let mut out = vec![];
            agg.flush_into(&mut out);
            assert_eq!(1, out.len(), "each bucket should flush exactly one metric");
            // Step forward by one interval so the next iteration uses a fresh bucket.
            bucket_key += interval_i64;
        }

        assert_eq!(
            agg.event_time_prev_buckets.len(),
            0,
            "non-Diff mode must not retain any previous bucket"
        );
    }

    /// `Diff` mode needs the previous bucket to compute deltas, but the
    /// rolling window must stay bounded rather than grow with every flush.
    #[test]
    fn event_time_diff_mode_retains_only_bounded_prev_buckets() {
        let interval_ms = 10_000_u64;

        let mut agg = Aggregate::new(&AggregateConfig {
            interval_ms,
            mode: AggregationMode::Diff,
            time_source: TimeSource::EventTime,
            allowed_lateness_ms: 0,
            use_system_time_for_missing_timestamps: false,
            max_future_ms: 10000,
        })
        .unwrap();

        let now_ms = Utc::now().timestamp_millis();
        let interval_i64 = interval_ms as i64;
        let mut bucket_key = (now_ms / interval_i64) * interval_i64 - interval_i64 * 100;

        for _ in 0..50 {
            let ts = Utc
                .timestamp_millis_opt(bucket_key + interval_i64 / 2)
                .latest()
                .unwrap();
            agg.record(make_metric_with_timestamp(
                "diff_probe",
                MetricKind::Absolute,
                MetricValue::Gauge { value: 1.0 },
                ts,
            ));
            let mut out = vec![];
            agg.flush_into(&mut out);
            bucket_key += interval_i64;
        }

        assert!(
            agg.event_time_prev_buckets.len() <= 2,
            "Diff mode must keep at most a small rolling window of previous buckets, \
             got {}",
            agg.event_time_prev_buckets.len()
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
allowed_lateness_ms = 600000
time_source = "EventTime"
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
            assert_eq!(ev.as_metric().series().name.name.as_str(), "shutdown_drain_probe");
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
        let mut agg = Aggregate::new(&AggregateConfig {
            interval_ms: 10000_u64,
            mode: AggregationMode::Auto,
            time_source: TimeSource::EventTime,
            allowed_lateness_ms: 0,
            use_system_time_for_missing_timestamps: false,
            max_future_ms: 5000, // only allow 5 seconds into the future
        })
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

    #[test]
    fn event_time_missing_timestamp_uses_system_time() {
        let mut agg = Aggregate::new(&AggregateConfig {
            interval_ms: 10000_u64,
            mode: AggregationMode::Auto,
            time_source: TimeSource::EventTime,
            allowed_lateness_ms: 0,
            use_system_time_for_missing_timestamps: true,
            max_future_ms: 10000,
        })
        .unwrap();

        // Event with no timestamp — should be bucketed using system time and accepted.
        let event = make_metric(
            "counter_no_ts",
            MetricKind::Incremental,
            MetricValue::Counter { value: 7.0 },
        );
        agg.record(event);

        // The event goes into the current (system-time) bucket, which hasn't completed yet,
        // so it should NOT flush on the first call.
        let mut out = vec![];
        agg.flush_into(&mut out);
        assert_eq!(
            0,
            out.len(),
            "Current bucket not yet flushed — event is pending"
        );

        // Verify the event is actually stored (not dropped): the bucket exists.
        assert_eq!(
            1,
            agg.event_time_buckets.len(),
            "Event should be stored in exactly one bucket"
        );
    }

    #[test]
    fn event_time_diff_first_bucket_no_previous() {
        let mut agg = Aggregate::new(&AggregateConfig {
            interval_ms: 10000_u64,
            mode: AggregationMode::Diff,
            time_source: TimeSource::EventTime,
            allowed_lateness_ms: 0,
            use_system_time_for_missing_timestamps: false,
            max_future_ms: 10000,
        })
        .unwrap();

        let base_time = DateTime::parse_from_rfc3339("2025-12-29T11:00:20Z")
            .unwrap()
            .with_timezone(&Utc);

        let event = make_metric_with_timestamp(
            "diff_gauge",
            MetricKind::Absolute,
            MetricValue::Gauge { value: 42.0 },
            base_time,
        );
        agg.record(event);

        let mut out = vec![];
        agg.flush_into(&mut out);

        // First bucket has no previous window — should emit raw value without subtraction.
        assert_eq!(1, out.len());
        if let MetricValue::Gauge { value } = out[0].as_metric().value() {
            assert_eq!(*value, 42.0, "First bucket in Diff emits the raw value");
        } else {
            panic!("Expected Gauge metric value");
        }
    }

    #[test]
    fn event_time_count_mode() {
        let mut agg = Aggregate::new(&AggregateConfig {
            interval_ms: 10000_u64,
            mode: AggregationMode::Count,
            time_source: TimeSource::EventTime,
            allowed_lateness_ms: 0,
            use_system_time_for_missing_timestamps: false,
            max_future_ms: 10000,
        })
        .unwrap();

        let base_time = DateTime::parse_from_rfc3339("2025-12-29T11:00:20Z")
            .unwrap()
            .with_timezone(&Utc);

        // Three events (any kind) in the same bucket should produce count = 3.
        for i in 0..3 {
            let event = make_metric_with_timestamp(
                "count_metric",
                MetricKind::Incremental,
                MetricValue::Counter { value: 1.0 },
                base_time + chrono::Duration::seconds(i),
            );
            agg.record(event);
        }

        let mut out = vec![];
        agg.flush_into(&mut out);

        assert_eq!(1, out.len());
        if let MetricValue::Counter { value } = out[0].as_metric().value() {
            assert_eq!(*value, 3.0, "Count mode should count 3 events");
        } else {
            panic!("Expected Counter value from Count mode");
        }
    }

    #[test]
    fn event_time_watermark_only_advances() {
        let mut agg = Aggregate::new(&AggregateConfig {
            interval_ms: 10000_u64,
            mode: AggregationMode::Auto,
            time_source: TimeSource::EventTime,
            allowed_lateness_ms: 0,
            use_system_time_for_missing_timestamps: false,
            max_future_ms: 10000,
        })
        .unwrap();

        let base_time = DateTime::parse_from_rfc3339("2025-12-29T11:00:20Z")
            .unwrap()
            .with_timezone(&Utc);

        // Arrive in reverse bucket order: newer first, then older.
        let newer = make_metric_with_timestamp(
            "counter_a",
            MetricKind::Incremental,
            MetricValue::Counter { value: 1.0 },
            base_time + chrono::Duration::seconds(30), // bucket 3
        );
        let older = make_metric_with_timestamp(
            "counter_a",
            MetricKind::Incremental,
            MetricValue::Counter { value: 2.0 },
            base_time, // bucket 1
        );

        // Flush the newer bucket first so that watermark advances to bucket 3.
        agg.record(newer);
        let mut out = vec![];
        agg.flush_into(&mut out);
        let watermark_after_newer = agg.watermark;
        assert!(watermark_after_newer.is_some());

        // Now record the older bucket — it is below watermark and should be rejected.
        agg.record(older);
        out.clear();
        agg.flush_into(&mut out);
        assert_eq!(0, out.len(), "Older bucket below watermark must be rejected");

        // Watermark must not have regressed.
        assert_eq!(
            agg.watermark, watermark_after_newer,
            "Watermark must never decrease"
        );
    }

    #[test]
    fn event_time_empty_flush_produces_nothing() {
        let mut agg = Aggregate::new(&AggregateConfig {
            interval_ms: 10000_u64,
            mode: AggregationMode::Auto,
            time_source: TimeSource::EventTime,
            allowed_lateness_ms: 0,
            use_system_time_for_missing_timestamps: false,
            max_future_ms: 10000,
        })
        .unwrap();

        let mut out = vec![];
        agg.flush_into(&mut out);
        assert_eq!(0, out.len(), "Flush with no events must produce no output");

        // Second call must also be safe.
        agg.flush_into(&mut out);
        assert_eq!(0, out.len());
    }

    #[test]
    fn event_time_multiple_different_metrics_same_bucket() {
        let mut agg = Aggregate::new(&AggregateConfig {
            interval_ms: 10000_u64, // 10 seconds
            mode: AggregationMode::Auto,
            time_source: TimeSource::EventTime,
            allowed_lateness_ms: 0,
            use_system_time_for_missing_timestamps: false,
            max_future_ms: 10000,
        })
        .unwrap();

        let base_time = DateTime::parse_from_rfc3339("2025-12-29T11:00:20Z")
            .unwrap()
            .with_timezone(&Utc);

        // Create multiple different metrics (different names) in the same time bucket
        let counter_a = make_metric_with_timestamp(
            "counter_a",
            MetricKind::Incremental,
            MetricValue::Counter { value: 10.0 },
            base_time,
        );
        let counter_b = make_metric_with_timestamp(
            "counter_b",
            MetricKind::Incremental,
            MetricValue::Counter { value: 20.0 },
            base_time + chrono::Duration::seconds(3), // Still in same bucket
        );
        let gauge_c = make_metric_with_timestamp(
            "gauge_c",
            MetricKind::Absolute,
            MetricValue::Gauge { value: 30.0 },
            base_time + chrono::Duration::seconds(7), // Still in same bucket
        );

        // Record all metrics in the same bucket
        agg.record(counter_a);
        agg.record(counter_b);
        agg.record(gauge_c);

        let mut out = vec![];
        agg.flush_into(&mut out);

        // All three different metrics should be flushed together from the same bucket
        assert_eq!(
            3,
            out.len(),
            "Should flush all three different metrics from the same bucket"
        );

        // Verify we have all three metrics
        let mut found_counter_a = false;
        let mut found_counter_b = false;
        let mut found_gauge_c = false;

        for event in out {
            let metric = event.as_metric();
            match metric.series().name.name.as_str() {
                "counter_a" => {
                    found_counter_a = true;
                    if let MetricValue::Counter { value } = metric.value() {
                        assert_eq!(*value, 10.0);
                    }
                }
                "counter_b" => {
                    found_counter_b = true;
                    if let MetricValue::Counter { value } = metric.value() {
                        assert_eq!(*value, 20.0);
                    }
                }
                "gauge_c" => {
                    found_gauge_c = true;
                    if let MetricValue::Gauge { value } = metric.value() {
                        assert_eq!(*value, 30.0);
                    }
                }
                _ => panic!("Unexpected metric name: {}", metric.series().name.name),
            }
        }

        assert!(found_counter_a, "Should have found counter_a");
        assert!(found_counter_b, "Should have found counter_b");
        assert!(found_gauge_c, "Should have found gauge_c");
    }

    /// An event whose (kind, value) is incompatible with the configured
    /// aggregation mode (for example an `Incremental` event arriving at a
    /// `Mean`-configured aggregator) is dropped explicitly:
    ///
    ///   * No bucket is created in either `event_time_buckets` or
    ///     `event_time_multi_buckets`.
    ///   * The watermark is unaffected, so a subsequent valid event for an
    ///     earlier bucket is still accepted.
    ///
    /// This guards against a stray no-op event silently rejecting valid
    /// in-order data for earlier windows.
    #[test]
    fn event_time_discarded_events_do_not_advance_watermark() {
        let interval_ms = 10_000_u64;

        let mut agg = Aggregate::new(&AggregateConfig {
            interval_ms,
            mode: AggregationMode::Mean,
            time_source: TimeSource::EventTime,
            allowed_lateness_ms: 0,
            use_system_time_for_missing_timestamps: false,
            // Generous future allowance so the stray event isn't rejected by
            // `max_future_ms` -- we want it to fail the *mode* compatibility
            // check, not the future check.
            max_future_ms: 600_000,
        })
        .unwrap();

        let now_ms = Utc::now().timestamp_millis();
        let interval_i64 = interval_ms as i64;

        // Two buckets far enough in the past that the wall-clock predicate
        // would flush them.
        let stray_bucket = (now_ms / interval_i64) * interval_i64 - interval_i64 * 100;
        let earlier_bucket = stray_bucket - interval_i64;

        let stray_ts = Utc
            .timestamp_millis_opt(stray_bucket + interval_i64 / 2)
            .latest()
            .unwrap();
        let earlier_ts = Utc
            .timestamp_millis_opt(earlier_bucket + interval_i64 / 2)
            .latest()
            .unwrap();

        // An Incremental event in `Mean` mode is incompatible and must be
        // dropped without leaving any bucket behind.
        let stray = make_metric_with_timestamp(
            "watermark_probe",
            MetricKind::Incremental,
            MetricValue::Counter { value: 1.0 },
            stray_ts,
        );
        agg.record(stray);

        assert!(
            agg.event_time_buckets.is_empty(),
            "stray Incremental event in Mean mode must NOT create an event_time_buckets entry"
        );
        assert!(
            agg.event_time_multi_buckets.is_empty(),
            "stray Incremental event in Mean mode must NOT create an event_time_multi_buckets entry"
        );

        // Flushing must not advance the watermark, because nothing was recorded.
        let mut out = vec![];
        agg.flush_into(&mut out);
        assert_eq!(out.len(), 0, "stray event must produce no flush output");
        assert!(
            agg.watermark.is_none(),
            "watermark must remain None when no real data has been flushed"
        );

        // A valid Absolute Gauge event for an EARLIER bucket must still be
        // accepted -- a stray no-op event must not cause the watermark to
        // reject otherwise-valid data.
        let valid = make_metric_with_timestamp(
            "watermark_probe",
            MetricKind::Absolute,
            MetricValue::Gauge { value: 42.0 },
            earlier_ts,
        );
        agg.record(valid);

        out.clear();
        agg.flush_into(&mut out);
        assert_eq!(
            out.len(),
            1,
            "valid earlier event must not be rejected by a stray-event watermark"
        );
        if let MetricValue::Gauge { value } = out[0].as_metric().value() {
            assert_eq!(*value, 42.0);
        } else {
            panic!("Expected Gauge metric value");
        }
    }
}
