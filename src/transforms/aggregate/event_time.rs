use std::collections::{HashMap, hash_map::Entry};

use chrono::{DateTime, Utc};
use vector_lib::event::{
    MetricValue,
    metric::{Metric, MetricData, MetricKind, MetricSeries},
};

use super::AggregationMode;
use super::config::{EventTimeConfig, MissingTimestamp};
use super::transform::{Aggregate, BucketKey, MetricEntry};
use crate::{
    event::{Event, EventMetadata},
    internal_events::{
        AggregateEventDropped, AggregateEventRecorded, AggregateFlushed, AggregateUpdateFailed,
    },
};

impl Aggregate {
    /// Event-time settings; only called from the event-time path.
    fn event_time(&self) -> &EventTimeConfig {
        self.config
            .event_time
            .as_ref()
            .expect("event-time path requires AggregateConfig.event_time")
    }

    pub(crate) fn record_event_time(
        &mut self,
        series: MetricSeries,
        mut data: MetricData,
        metadata: EventMetadata,
        timestamp: Option<DateTime<Utc>>,
    ) -> Option<Event> {
        // Mirror the system-time passthrough arms before any timestamp gating.
        if Self::passes_through_unchanged(self.config.mode, &data) {
            return Some(Event::Metric(Metric::from_parts(series, data, metadata)));
        }

        // Mean/Stdev ignore absolute non-gauge values (no passthrough, no bucket).
        if Self::is_silently_ignored(self.config.mode, &data) {
            emit!(AggregateEventRecorded);
            return None;
        }

        let event_time = *self.event_time();

        // Capture one wall-clock instant for fallback synthesis and all subsequent
        // skew/cutoff checks so a missing timestamp cannot be assigned to bucket B
        // via `now` and then rejected because a later `Utc::now()` crosses B's end.
        let now = Utc::now();
        let now_ms = now.timestamp_millis();

        let ts = match timestamp {
            Some(ts) => ts,
            None => match event_time.missing_timestamp {
                MissingTimestamp::UseSystemTime => now,
                MissingTimestamp::Drop => {
                    emit!(AggregateEventDropped {
                        reason: "Event missing timestamp required for event-time aggregation."
                    });
                    return None;
                }
            },
        };
        // Preserve (or synthesize) the timestamp in the stored metric so that
        // event-time "latest" selection can compare timestamps reliably.
        data.time.timestamp = Some(ts);

        if event_time.max_future_ms > 0 {
            let max_future_ms = i64::try_from(event_time.max_future_ms)
                .expect("max_future_ms validated to fit in i64 in Aggregate::new");
            let drift_ms = ts.timestamp_millis().saturating_sub(now_ms);
            if drift_ms > max_future_ms {
                emit!(AggregateEventDropped {
                    reason: "Event timestamp too far in the future."
                });
                return None;
            }
        }

        let bucket_key = self.bucket_key(ts);

        if self.is_too_late(bucket_key) {
            emit!(AggregateEventDropped {
                reason: "Event timestamp is too late; bucket already flushed."
            });
            return None;
        }

        if self.is_past_bucket_cutoff(bucket_key, now_ms) {
            emit!(AggregateEventDropped {
                reason: "Event timestamp is too late; bucket window has ended."
            });
            return None;
        }

        debug_assert!(
            Self::will_be_stored(self.config.mode, &data),
            "only storable metrics reach event-time bucketing"
        );
        self.record_into_bucket(bucket_key, series, data, metadata);
        emit!(AggregateEventRecorded);
        None
    }

    /// Start of the half-open window `[bucket_key, bucket_key + interval_ms)` containing
    /// `timestamp`, aligned to multiples of `interval_ms` from the Unix epoch.
    ///
    /// Euclidean division (`div_euclid`) is required: Rust's truncating `/`
    /// rounds toward zero, so timestamps just before the epoch (negative
    /// millis) would incorrectly map into the non-negative bucket `[0, interval)`
    /// instead of `[-interval, 0)`.
    pub(crate) fn bucket_key(&self, timestamp: DateTime<Utc>) -> BucketKey {
        let timestamp_ms = timestamp.timestamp_millis();
        // Range-validated in `Aggregate::new` to fit in i64.
        let interval_ms = i64::try_from(self.config.interval_ms)
            .expect("interval_ms validated to fit in i64 in Aggregate::new");
        timestamp_ms
            .div_euclid(interval_ms)
            .saturating_mul(interval_ms)
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

    /// Returns `true` when `now_ms` is at or past the end of `bucket_key`'s
    /// window plus `allowed_lateness_ms` — the same predicate used to decide
    /// flush eligibility in `flush_event_time_buckets`. Recording must reject
    /// events once this cutoff passes even if the periodic flush tick has not
    /// run yet, so strict lateness is not weakened by a long flush interval.
    pub(crate) fn is_past_bucket_cutoff(&self, bucket_key: BucketKey, now_ms: i64) -> bool {
        let interval_ms = i64::try_from(self.config.interval_ms)
            .expect("interval_ms validated to fit in i64 in Aggregate::new");
        let grace_ms = i64::try_from(self.event_time().allowed_lateness_ms)
            .expect("allowed_lateness_ms validated to fit in i64 in Aggregate::new");
        now_ms
            >= bucket_key
                .saturating_add(interval_ms)
                .saturating_add(grace_ms)
    }
    /// Records an event-time event into the appropriate bucket.
    ///
    /// Callers must reject incompatible events via `will_be_stored` before
    /// calling this function so stray events never allocate buckets.
    fn record_into_bucket(
        &mut self,
        bucket_key: BucketKey,
        series: MetricSeries,
        data: MetricData,
        metadata: EventMetadata,
    ) {
        let mode = self.config.mode;

        match mode {
            AggregationMode::Auto => {
                let bucket = self.event_time_buckets.entry(bucket_key).or_default();
                match data.kind {
                    MetricKind::Incremental => {
                        Self::record_sum_in_map(bucket, series, data, metadata);
                    }
                    MetricKind::Absolute => match bucket.entry(series) {
                        Entry::Vacant(entry) => {
                            entry.insert((data, metadata));
                        }
                        Entry::Occupied(mut entry) => {
                            if entry.get().0.kind != data.kind {
                                emit!(AggregateUpdateFailed);
                                let existing = entry.get_mut();
                                existing.1.merge(metadata);
                                existing.0 = data;
                            } else {
                                // In event-time mode, "latest" means latest *event timestamp*
                                // within the time bucket, not latest arrival order.
                                Self::select_latest_by_event_timestamp(
                                    entry.get_mut(),
                                    data,
                                    metadata,
                                );
                            }
                        }
                    },
                }
            }
            AggregationMode::Sum => {
                let bucket = self.event_time_buckets.entry(bucket_key).or_default();
                Self::record_sum_in_map(bucket, series, data, metadata);
            }
            AggregationMode::Latest | AggregationMode::Diff => {
                // `data.kind == Absolute` is guaranteed by `will_be_stored`.
                let bucket = self.event_time_buckets.entry(bucket_key).or_default();
                match bucket.entry(series) {
                    Entry::Vacant(entry) => {
                        entry.insert((data, metadata));
                    }
                    Entry::Occupied(mut entry) => {
                        Self::select_latest_by_event_timestamp(entry.get_mut(), data, metadata);
                    }
                }
            }
            AggregationMode::Count => {
                let bucket = self.event_time_buckets.entry(bucket_key).or_default();
                Self::record_count_in_map(bucket, series, data, metadata);
            }
            AggregationMode::Max | AggregationMode::Min => {
                let bucket = self.event_time_buckets.entry(bucket_key).or_default();
                Self::record_comparison_in_map(bucket, series, data, metadata, mode);
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
    }

    /// Selects the metric data with the latest event timestamp while merging
    /// `EventMetadata` from every consumed sample (acknowledgements / secrets).
    fn select_latest_by_event_timestamp(
        existing: &mut MetricEntry,
        data: MetricData,
        metadata: EventMetadata,
    ) {
        let new_ts = data.timestamp().cloned();
        let existing_ts = existing.0.timestamp().cloned();
        let should_replace = match (new_ts, existing_ts) {
            (Some(n), Some(e)) => n >= e,
            (Some(_), None) => true,
            _ => false,
        };
        existing.1.merge(metadata);
        if should_replace {
            existing.0 = data;
        }
    }

    fn record_sum_in_map(
        map: &mut HashMap<MetricSeries, MetricEntry>,
        series: MetricSeries,
        data: MetricData,
        metadata: EventMetadata,
    ) {
        match data.kind {
            MetricKind::Incremental => match map.entry(series) {
                Entry::Occupied(mut entry) => {
                    let existing = entry.get_mut();
                    // In order to update (add) the new and old kind's must match.
                    // Metadata is always merged, even on a kind mismatch, so an
                    // existing sample's finalizers are never discarded.
                    let updated = existing.0.kind == data.kind && existing.0.update(&data);
                    existing.1.merge(metadata);
                    if !updated {
                        emit!(AggregateUpdateFailed);
                        existing.0 = data;
                    }
                }
                Entry::Vacant(entry) => {
                    entry.insert((data, metadata));
                }
            },
            MetricKind::Absolute => {}
        }
    }

    fn record_count_in_map(
        map: &mut HashMap<MetricSeries, MetricEntry>,
        series: MetricSeries,
        mut data: MetricData,
        metadata: EventMetadata,
    ) {
        let mut count_data = data.clone();
        let existing = map.entry(series).or_insert_with(|| {
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

    fn record_comparison_in_map(
        map: &mut HashMap<MetricSeries, MetricEntry>,
        series: MetricSeries,
        data: MetricData,
        metadata: EventMetadata,
        mode: AggregationMode,
    ) {
        match data.kind {
            MetricKind::Incremental => (),
            MetricKind::Absolute => match map.entry(series) {
                Entry::Occupied(mut entry) => {
                    let existing = entry.get_mut();
                    // In order to update (add) the new and old kind's must match
                    if existing.0.kind == data.kind {
                        // Determine the winner before touching `existing` so the
                        // comparison borrow of `existing.0.value()` ends here;
                        // metadata is then merged unconditionally below so a
                        // losing sample's finalizers are not discarded.
                        let should_update = match (existing.0.value(), data.value(), mode) {
                            (
                                MetricValue::Gauge {
                                    value: existing_value,
                                },
                                MetricValue::Gauge { value: new_value },
                                AggregationMode::Max,
                            ) => new_value > existing_value,
                            (
                                MetricValue::Gauge {
                                    value: existing_value,
                                },
                                MetricValue::Gauge { value: new_value },
                                AggregationMode::Min,
                            ) => new_value < existing_value,
                            _ => false,
                        };
                        existing.1.merge(metadata);
                        if should_update {
                            existing.0 = data;
                        }
                    } else {
                        emit!(AggregateUpdateFailed);
                        existing.1.merge(metadata);
                        existing.0 = data;
                    }
                }
                Entry::Vacant(entry) => {
                    entry.insert((data, metadata));
                }
            },
        }
    }
    pub(crate) fn flush_event_time_buckets(&mut self, output: &mut Vec<Event>, force: bool) {
        let now = Utc::now();
        let now_ms = now.timestamp_millis();
        // Range-validated in `Aggregate::new` to fit in i64.
        let interval_ms = i64::try_from(self.config.interval_ms)
            .expect("interval_ms validated to fit in i64 in Aggregate::new");
        let grace_ms = i64::try_from(self.event_time().allowed_lateness_ms)
            .expect("allowed_lateness_ms validated to fit in i64 in Aggregate::new");

        // `bucket_key + interval_ms + grace_ms` is i64 arithmetic. With
        // `max_future_ms = 0` (documented as accepting arbitrary future
        // timestamps), a metric near `DateTime::<Utc>::MAX_UTC` can produce
        // a `bucket_key` close to `i64::MAX` and plain `+` would either
        // panic (overflow-checked builds) or wrap negative and flush the
        // bucket immediately -- the wrap then advances the watermark to
        // near `i64::MAX` and every subsequent normal event is rejected as
        // late. Saturating addition keeps far-future buckets parked at
        // `i64::MAX` (never eligible until `force`) instead.
        let buckets_to_flush: Vec<BucketKey> = self
            .event_time_buckets
            .keys()
            .filter(|&&bucket_key| {
                force
                    || now_ms
                        >= bucket_key
                            .saturating_add(interval_ms)
                            .saturating_add(grace_ms)
            })
            .copied()
            .collect();

        for bucket_key in buckets_to_flush {
            if let Some(bucket_map) = self.event_time_buckets.remove(&bucket_key) {
                // Diff mode must retain `bucket_map` to subtract against the
                // next flush, so it iterates by reference and per-entry clones
                // only what `Metric::from_parts` consumes. Other modes never
                // touch the map again, so they consume it directly — avoiding
                // the full `HashMap` allocation and per-entry copy that the
                // previous unconditional `bucket_map.clone()` performed on
                // every flush (significant under high-cardinality event-time
                // workloads).
                if matches!(self.config.mode, AggregationMode::Diff) {
                    let prev_bucket_key = bucket_key.saturating_sub(interval_ms);
                    let mut prev_data = HashMap::with_capacity(bucket_map.len());
                    for (series, (data, metadata)) in bucket_map {
                        let mut metric = Metric::from_parts(series.clone(), data.clone(), metadata);
                        if let Some(prev_bucket) =
                            self.event_time_prev_buckets.get(&prev_bucket_key)
                            && let Some(prev_entry) = prev_bucket.get(metric.series())
                            && metric.data().kind == prev_entry.kind
                            && !metric.subtract(prev_entry)
                        {
                            emit!(AggregateUpdateFailed);
                        }
                        output.push(Event::Metric(metric));
                        // Retain data only — drop metadata/finalizers so
                        // acknowledgements are not held after emission.
                        prev_data.insert(series, data);
                    }

                    self.event_time_prev_buckets.insert(bucket_key, prev_data);
                    // Keep only a small rolling window for diffing against the
                    // immediately preceding bucket.
                    let min_keep = bucket_key.saturating_sub(interval_ms);
                    self.event_time_prev_buckets.retain(|&k, _| k >= min_keep);
                } else {
                    for (series, entry) in bucket_map {
                        let metric = Metric::from_parts(series, entry.0, entry.1);
                        output.push(Event::Metric(metric));
                    }
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
