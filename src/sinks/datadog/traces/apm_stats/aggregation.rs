use std::{collections::BTreeMap, sync::Arc};

use chrono::Utc;
use vrl::event_path;

use super::{
    bucket::Bucket, ClientStatsBucket, ClientStatsPayload, PartitionKey,
    BUCKET_DURATION_NANOSECONDS,
};
use crate::event::{ObjectMap, TraceEvent, Value};

const MEASURED_KEY: &str = "_dd.measured";
const PARTIAL_VERSION_KEY: &str = "_dd.partial_version";
const TAG_STATUS_CODE: &str = "http.status_code";
const TAG_SYNTHETICS: &str = "synthetics";
const TOP_LEVEL_KEY: &str = "_top_level";

/// The number of bucket durations to keep in memory before flushing them.
const BUCKET_WINDOW_LEN: u64 = 2;

#[derive(PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct AggregationKey {
    pub(crate) payload_key: PayloadAggregationKey,
    pub(crate) bucket_key: BucketAggregationKey,
}

impl AggregationKey {
    fn new_aggregation_from_span(
        span: &ObjectMap,
        payload_key: PayloadAggregationKey,
        synthetics: bool,
    ) -> Self {
        AggregationKey {
            payload_key: payload_key.with_span_context(span),
            bucket_key: BucketAggregationKey {
                service: span
                    .get("service")
                    .map(|v| v.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                name: span
                    .get("name")
                    .map(|v| v.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                resource: span
                    .get("resource")
                    .map(|v| v.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                ty: span
                    .get("type")
                    .map(|v| v.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                status_code: span
                    .get("meta")
                    .and_then(|m| m.as_object())
                    .and_then(|m| m.get(TAG_STATUS_CODE))
                    // the meta field is supposed to be a string/string map
                    .and_then(|s| s.to_string_lossy().parse::<u32>().ok())
                    .unwrap_or_default(),
                synthetics,
            },
        }
    }
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct PayloadAggregationKey {
    pub(crate) env: String,
    pub(crate) hostname: String,
    pub(crate) version: String,
    pub(crate) container_id: String,
}

impl PayloadAggregationKey {
    fn with_span_context(self, span: &ObjectMap) -> Self {
        PayloadAggregationKey {
            env: span
                .get("meta")
                .and_then(|m| m.as_object())
                .and_then(|m| m.get("env"))
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or(self.env),
            hostname: self.hostname,
            version: self.version,
            container_id: self.container_id,
        }
    }
}

#[derive(PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct BucketAggregationKey {
    pub(crate) service: String,
    pub(crate) name: String,
    pub(crate) resource: String,
    pub(crate) ty: String,
    pub(crate) status_code: u32,
    pub(crate) synthetics: bool,
}

pub struct Aggregator {
    /// The key represents the timestamp (in nanoseconds) of the beginning of the time window (that lasts 10 seconds) on
    /// which the associated bucket will calculate statistics.
    buckets: BTreeMap<u64, Bucket>,

    /// The oldest timestamp we will allow for the current time bucket.
    oldest_timestamp: u64,

    /// Env associated with the Agent.
    agent_env: Option<String>,

    /// Hostname associated with the Agent.
    agent_hostname: Option<String>,

    /// Version associated with the Agent.
    agent_version: Option<String>,

    /// API key associated with the Agent.
    api_key: Option<Arc<str>>,

    /// Default API key to use if api_key not set.
    default_api_key: Arc<str>,
}

impl Aggregator {
    pub fn new(default_api_key: Arc<str>) -> Self {
        Self {
            buckets: BTreeMap::new(),
            oldest_timestamp: align_timestamp(
                Utc::now()
                    .timestamp_nanos_opt()
                    .expect("Timestamp out of range") as u64,
            ),
            default_api_key,
            // We can't know the below fields until have received a trace event
            agent_env: None,
            agent_hostname: None,
            agent_version: None,
            api_key: None,
        }
    }

    /// Updates cached properties from the Agent.
    pub(crate) fn update_agent_properties(&mut self, partition_key: &PartitionKey) {
        if self.agent_env.is_none() {
            if let Some(env) = &partition_key.env {
                self.agent_env = Some(env.clone());
            }
        }
        if self.agent_hostname.is_none() {
            if let Some(hostname) = &partition_key.hostname {
                self.agent_hostname = Some(hostname.clone());
            }
        }
        if self.agent_version.is_none() {
            if let Some(version) = &partition_key.agent_version {
                self.agent_version = Some(version.clone());
            }
        }
        if self.api_key.is_none() {
            if let Some(api_key) = &partition_key.api_key {
                self.api_key = Some(Arc::<str>::clone(api_key));
            }
        }
    }

    pub(crate) fn get_agent_env(&self) -> String {
        self.agent_env.clone().unwrap_or_default()
    }

    pub(crate) fn get_agent_hostname(&self) -> String {
        self.agent_hostname.clone().unwrap_or_default()
    }

    pub(crate) fn get_agent_version(&self) -> String {
        self.agent_version.clone().unwrap_or_default()
    }

    pub(crate) fn get_api_key(&self) -> Arc<str> {
        self.api_key
            .clone()
            .unwrap_or_else(|| Arc::clone(&self.default_api_key))
    }

    /// Iterates over a trace's constituting spans and upon matching conditions it updates statistics (mostly using the top level span).
    pub(crate) fn handle_trace(&mut self, partition_key: &PartitionKey, trace: &TraceEvent) {
        // Based on https://github.com/DataDog/datadog-agent/blob/cfa750c7412faa98e87a015f8ee670e5828bbe7f/pkg/trace/stats/concentrator.go#L148-L184

        let spans = match trace.get(event_path!("spans")) {
            Some(Value::Array(v)) => v.iter().filter_map(|s| s.as_object()).collect(),
            _ => vec![],
        };

        let weight = super::weight::extract_weight_from_root_span(&spans);
        let payload_aggkey = PayloadAggregationKey {
            env: partition_key.env.clone().unwrap_or_default(),
            hostname: partition_key.hostname.clone().unwrap_or_default(),
            version: trace
                .get(event_path!("app_version"))
                .map(|v| v.to_string_lossy().into_owned())
                .unwrap_or_default(),
            container_id: trace
                .get(event_path!("container_id"))
                .map(|v| v.to_string_lossy().into_owned())
                .unwrap_or_default(),
        };
        let synthetics = trace
            .get(event_path!("origin"))
            .map(|v| v.to_string_lossy().starts_with(TAG_SYNTHETICS))
            .unwrap_or(false);

        spans.iter().for_each(|span| {
            let is_top = has_top_level(span);
            if !(is_top || is_measured(span) || is_partial_snapshot(span)) {
                return;
            }

            self.handle_span(span, weight, is_top, synthetics, payload_aggkey.clone());
        });
    }

    /// Aggregates statistics per key over 10 seconds windows.
    /// The key is constructed from various span/trace properties (see `AggregationKey`).
    fn handle_span(
        &mut self,
        span: &ObjectMap,
        weight: f64,
        is_top: bool,
        synthetics: bool,
        payload_aggkey: PayloadAggregationKey,
    ) {
        // Based on: https://github.com/DataDog/datadog-agent/blob/cfa750c7412faa98e87a015f8ee670e5828bbe7f/pkg/trace/stats/statsraw.go#L147-L182

        let aggkey = AggregationKey::new_aggregation_from_span(span, payload_aggkey, synthetics);

        let start = match span.get("start") {
            Some(Value::Timestamp(val)) => {
                val.timestamp_nanos_opt().expect("Timestamp out of range") as u64
            }
            _ => Utc::now()
                .timestamp_nanos_opt()
                .expect("Timestamp out of range") as u64,
        };

        let duration = match span.get("duration") {
            Some(Value::Integer(val)) => *val as u64,
            None => 0,
            _ => panic!("`duration` should be an i64"),
        };

        let end = start + duration;

        // 10 second bucket window
        let mut btime = align_timestamp(end);

        // If too far in the past, use the oldest-allowed time bucket instead
        if btime < self.oldest_timestamp {
            btime = self.oldest_timestamp
        }

        match self.buckets.get_mut(&btime) {
            Some(b) => {
                b.add(span, weight, is_top, aggkey);
            }
            None => {
                let mut b = Bucket {
                    start: btime,
                    duration: BUCKET_DURATION_NANOSECONDS,
                    data: BTreeMap::new(),
                };
                b.add(span, weight, is_top, aggkey);

                debug!("Created {} start_time bucket.", btime);
                self.buckets.insert(btime, b);
            }
        }
    }

    /// Flushes the bucket cache.
    /// We cache and can compute stats only for the last `BUCKET_WINDOW_LEN * BUCKET_DURATION_NANOSECONDS` and after such time,
    /// buckets are then flushed. This only applies to past buckets. Stats buckets in the future are cached with no restriction.
    ///
    /// # Arguments
    ///
    /// * `force` - If true, all cached buckets are flushed.
    pub(crate) fn flush(&mut self, force: bool) -> Vec<ClientStatsPayload> {
        // Based on https://github.com/DataDog/datadog-agent/blob/cfa750c7412faa98e87a015f8ee670e5828bbe7f/pkg/trace/stats/concentrator.go#L38-L41
        // , and https://github.com/DataDog/datadog-agent/blob/cfa750c7412faa98e87a015f8ee670e5828bbe7f/pkg/trace/stats/concentrator.go#L195-L207

        let now = Utc::now()
            .timestamp_nanos_opt()
            .expect("Timestamp out of range") as u64;

        let flush_cutoff_time = if force {
            // flush all the remaining buckets (the Vector process is exiting)
            now
        } else {
            // maintain two buckets in the cache during normal operation
            now - (BUCKET_DURATION_NANOSECONDS * BUCKET_WINDOW_LEN)
        };

        let client_stats_payloads = self.get_client_stats_payloads(flush_cutoff_time);

        // update the oldest_timestamp allowed, to prevent having stats for an already flushed
        // bucket
        let new_oldest_ts =
            align_timestamp(now) - ((BUCKET_WINDOW_LEN - 1) * BUCKET_DURATION_NANOSECONDS);

        if new_oldest_ts > self.oldest_timestamp {
            debug!("Updated oldest_timestamp to {}.", new_oldest_ts);
            self.oldest_timestamp = new_oldest_ts;
        }

        client_stats_payloads
    }

    /// Builds the array of ClientStatsPayloads will be sent out as part of the StatsPayload.
    ///
    /// # Arguments
    ///
    /// * `flush_cutoff_time` - Timestamp in nanos to use to determine what buckets to keep in the cache and which to export.
    fn get_client_stats_payloads(&mut self, flush_cutoff_time: u64) -> Vec<ClientStatsPayload> {
        let client_stats_buckets = self.export_buckets(flush_cutoff_time);

        client_stats_buckets
            .into_iter()
            .map(|(payload_aggkey, csb)| {
                ClientStatsPayload {
                    env: payload_aggkey.env,
                    hostname: payload_aggkey.hostname,
                    container_id: payload_aggkey.container_id,
                    version: payload_aggkey.version,
                    stats: csb,
                    // All the following fields are left unset by the trace-agent:
                    // https://github.com/DataDog/datadog-agent/blob/42e72dd/pkg/trace/stats/concentrator.go#L216-L227
                    service: "".to_string(),
                    agent_aggregation: "".to_string(),
                    sequence: 0,
                    runtime_id: "".to_string(),
                    lang: "".to_string(),
                    tracer_version: "".to_string(),
                    tags: vec![],
                }
            })
            .collect::<Vec<ClientStatsPayload>>()
    }

    /// Exports the buckets that began before `flush_cutoff_time` and purges them from the cache.
    ///
    /// # Arguments
    ///
    /// * `flush_cutoff_time` - Timestamp in nanos to use to determine what buckets to keep in the cache and which to export.
    fn export_buckets(
        &mut self,
        flush_cutoff_time: u64,
    ) -> BTreeMap<PayloadAggregationKey, Vec<ClientStatsBucket>> {
        let mut m = BTreeMap::<PayloadAggregationKey, Vec<ClientStatsBucket>>::new();

        self.buckets.retain(|&bucket_start, bucket| {
            let retain = bucket_start > flush_cutoff_time;

            if !retain {
                debug!("Flushing {} start_time bucket.", bucket_start);

                bucket.export().into_iter().for_each(|(payload_key, csb)| {
                    match m.get_mut(&payload_key) {
                        None => {
                            m.insert(payload_key.clone(), vec![csb]);
                        }
                        Some(s) => {
                            s.push(csb);
                        }
                    };
                })
            }
            retain
        });

        m
    }
}

/// Returns the provided timestamp truncated to the bucket size.
/// This is the start time of the time bucket in which such timestamp falls.
const fn align_timestamp(start: u64) -> u64 {
    // Based on https://github.com/DataDog/datadog-agent/blob/cfa750c7412faa98e87a015f8ee670e5828bbe7f/pkg/trace/stats/concentrator.go#L232-L234
    start - (start % BUCKET_DURATION_NANOSECONDS)
}

/// Assumes that all metrics are all encoded as Value::Float.
/// Return the f64 of the specified key or None of key not present.
fn get_metric_value_float(span: &ObjectMap, key: &str) -> Option<f64> {
    span.get("metrics")
        .and_then(|m| m.as_object())
        .map(|m| match m.get(key) {
            Some(Value::Float(f)) => Some(f.into_inner()),
            None => None,
            _ => panic!("`metric` values should be all be f64"),
        })
        .unwrap_or(None)
}

/// Returns true if the value of this metric is equal to 1.0
fn metric_value_is_1(span: &ObjectMap, key: &str) -> bool {
    match get_metric_value_float(span, key) {
        Some(f) => f == 1.0,
        None => false,
    }
}

/// Returns true if span is top-level.
fn has_top_level(span: &ObjectMap) -> bool {
    // Based on: https://github.com/DataDog/datadog-agent/blob/cfa750c7412faa98e87a015f8ee670e5828bbe7f/pkg/trace/traceutil/span.go#L28-L31

    metric_value_is_1(span, TOP_LEVEL_KEY)
}

/// Returns true if a span should be measured (i.e. it should get trace metrics calculated).
fn is_measured(span: &ObjectMap) -> bool {
    // Based on https://github.com/DataDog/datadog-agent/blob/cfa750c7412faa98e87a015f8ee670e5828bbe7f/pkg/trace/traceutil/span.go#L40-L43

    metric_value_is_1(span, MEASURED_KEY)
}

/// Returns true if the span is a partial snapshot.
/// These types of spans are partial images of long-running spans.
/// When incomplete, a partial snapshot has a metric _dd.partial_version which is a positive integer.
/// The metric usually increases each time a new version of the same span is sent by the tracer
fn is_partial_snapshot(span: &ObjectMap) -> bool {
    // Based on: https://github.com/DataDog/datadog-agent/blob/cfa750c7412faa98e87a015f8ee670e5828bbe7f/pkg/trace/traceutil/span.go#L49-L52

    match get_metric_value_float(span, PARTIAL_VERSION_KEY) {
        Some(f) => f >= 0.0,
        None => false,
    }
}
