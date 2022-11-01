use std::{
    collections::BTreeMap,
    io::Write,
    sync::{Arc, Mutex},
};

use bytes::Bytes;
use chrono::Utc;
use prost::Message;
use serde::{Deserialize, Serialize};
use serde_bytes;
use snafu::ResultExt;
use tokio::sync::watch::Receiver;
use vector_common::{finalization::EventFinalizers, request_metadata::RequestMetadata};

use super::{
    build_request,
    config::{DatadogTracesEndpoint, DatadogTracesEndpointConfiguration},
    ddsketch_full,
    request_builder::{DDTracesMetadata, RequestBuilderError},
    sink::PartitionKey,
};
use crate::{
    event::{TraceEvent, Value},
    http::{BuildRequestSnafu, HttpClient},
    metrics::AgentDDSketch,
    sinks::util::{Compression, Compressor},
};

const MEASURED_KEY: &str = "_dd.measured";
const PARTIAL_VERSION_KEY: &str = "_dd.partial_version";
const SAMPLING_RATE_KEY: &str = "_sample_rate";
const TAG_STATUS_CODE: &str = "http.status_code";
const TAG_SYNTHETICS: &str = "synthetics";
const TOP_LEVEL_KEY: &str = "_top_level";

/// The duration of time in nanoseconds that a bucket covers.
const BUCKET_DURATION_NANOSECONDS: u64 = 10_000_000_000;

/// The number of bucket durations to keep in memory before flushing them.
const BUCKET_WINDOW_LEN: u64 = 2;

#[derive(PartialEq, Eq, PartialOrd, Ord)]
struct AggregationKey {
    payload_key: PayloadAggregationKey,
    bucket_key: BucketAggregationKey,
}

impl AggregationKey {
    fn new_aggregation_from_span(
        span: &BTreeMap<String, Value>,
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
struct PayloadAggregationKey {
    env: String,
    hostname: String,
    version: String,
    container_id: String,
}

impl PayloadAggregationKey {
    fn with_span_context(self, span: &BTreeMap<String, Value>) -> Self {
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
struct BucketAggregationKey {
    service: String,
    name: String,
    resource: String,
    ty: String,
    status_code: u32,
    synthetics: bool,
}

struct GroupedStats {
    hits: f64,
    top_level_hits: f64,
    errors: f64,
    duration: f64,
    ok_distribution: AgentDDSketch,
    err_distribution: AgentDDSketch,
}

impl GroupedStats {
    fn new() -> Self {
        GroupedStats {
            hits: 0.0,
            top_level_hits: 0.0,
            errors: 0.0,
            duration: 0.0,
            ok_distribution: AgentDDSketch::with_agent_defaults(),
            err_distribution: AgentDDSketch::with_agent_defaults(),
        }
    }

    fn export(&self, key: &AggregationKey) -> ClientGroupedStats {
        ClientGroupedStats {
            service: key.bucket_key.service.clone(),
            name: key.bucket_key.name.clone(),
            resource: key.bucket_key.resource.clone(),
            http_status_code: key.bucket_key.status_code,
            r#type: key.bucket_key.ty.clone(),
            db_type: "".to_string(),
            hits: self.hits.round() as u64,
            errors: self.errors.round() as u64,
            duration: self.duration.round() as u64,
            ok_summary: encode_sketch(&self.ok_distribution),
            error_summary: encode_sketch(&self.err_distribution),
            synthetics: key.bucket_key.synthetics,
            top_level_hits: self.top_level_hits.round() as u64,
        }
    }
}

/// Convert agent sketch variant to ./proto/dd_sketch_full.proto
fn encode_sketch(agent_sketch: &AgentDDSketch) -> Vec<u8> {
    // AgentDDSketch partitions the set of real numbers into intervals like [gamma^(n), gamma^(n+1)[,
    let index_mapping = ddsketch_full::IndexMapping {
        // This is the gamma value used to build the aforementioned partition scheme
        gamma: agent_sketch.gamma(),
        // This offset is applied to the powers of gamma to adjust sketch accuracy
        index_offset: agent_sketch.bin_index_offset() as f64,
        // Interpolation::None is the interpolation type as there is no interpolation when using the
        // aforementioned partition scheme
        interpolation: ddsketch_full::index_mapping::Interpolation::None as i32,
    };

    // zeroes depicts the number of values that falled around zero based on the sketch local accuracy
    // positives and negatives stores are repectively storing positive and negative values using the
    // exact same mechanism.
    let (positives, negatives, zeroes) = convert_stores(agent_sketch);
    let positives_store = ddsketch_full::Store {
        bin_counts: positives,
        contiguous_bin_counts: Vec::new(), // Empty as this not used for the current interpolation (Interpolation::None)
        contiguous_bin_index_offset: 0, // Empty as this not used for the current interpolation (Interpolation::None)
    };
    let negatives_store = ddsketch_full::Store {
        bin_counts: negatives,
        contiguous_bin_counts: Vec::new(), // Empty as this not used for the current interpolation (Interpolation::None)
        contiguous_bin_index_offset: 0, // Empty as this not used for the current interpolation (Interpolation::None)
    };
    ddsketch_full::DdSketch {
        mapping: Some(index_mapping),
        positive_values: Some(positives_store),
        negative_values: Some(negatives_store),
        zero_count: zeroes,
    }
    .encode_to_vec()
}

/// Split negative and positive values from an AgentDDSketch, also extract the number of values
/// that were accounted as 0.0.
fn convert_stores(agent_sketch: &AgentDDSketch) -> (BTreeMap<i32, f64>, BTreeMap<i32, f64>, f64) {
    let mut positives = BTreeMap::<i32, f64>::new();
    let mut negatives = BTreeMap::<i32, f64>::new();
    let mut zeroes = 0.0;
    let bin_map = agent_sketch.bin_map();
    bin_map
        .keys
        .into_iter()
        .zip(bin_map.counts.into_iter())
        .for_each(|(k, n)| {
            match k.signum() {
                0 => zeroes = n as f64,
                1 => {
                    positives.insert(k as i32, n as f64);
                }
                -1 => {
                    negatives.insert((-k) as i32, n as f64);
                }
                _ => {}
            };
        });
    (positives, negatives, zeroes)
}

/// Stores statistics for various `AggregationKey` in a given time window
struct Bucket {
    start: u64,
    duration: u64,
    data: BTreeMap<AggregationKey, GroupedStats>,
}

impl Bucket {
    fn export(&self) -> BTreeMap<PayloadAggregationKey, ClientStatsBucket> {
        let mut m = BTreeMap::<PayloadAggregationKey, ClientStatsBucket>::new();
        self.data.iter().for_each(|(k, v)| {
            let b = v.export(k);
            match m.get_mut(&k.payload_key) {
                None => {
                    let sb = ClientStatsBucket {
                        start: self.start,
                        duration: self.duration,
                        agent_time_shift: 0,
                        stats: vec![b],
                    };
                    m.insert(k.payload_key.clone(), sb);
                }
                Some(s) => {
                    s.stats.push(b);
                }
            };
        });
        m
    }

    fn add(
        &mut self,
        span: &BTreeMap<String, Value>,
        weight: f64,
        is_top: bool,
        aggkey: AggregationKey,
    ) {
        match self.data.get_mut(&aggkey) {
            Some(gs) => Bucket::update(span, weight, is_top, gs),
            None => {
                let mut gs = GroupedStats::new();
                Bucket::update(span, weight, is_top, &mut gs);
                self.data.insert(aggkey, gs);
            }
        }
    }

    /// Update a bucket with a new span. Computed statistics include the number of hits and the actual distribution of
    /// execution time, with isolated measurements for spans flagged as errored and spans without error.
    fn update(span: &BTreeMap<String, Value>, weight: f64, is_top: bool, gs: &mut GroupedStats) {
        is_top.then(|| {
            gs.top_level_hits += weight;
        });
        gs.hits += weight;
        let error = match span.get("error") {
            Some(Value::Integer(val)) => *val,
            None => 0,
            _ => panic!("`error` should be an i64"),
        };
        if error != 0 {
            gs.errors += weight;
        }
        let duration = match span.get("duration") {
            Some(Value::Integer(val)) => *val,
            None => 0,
            _ => panic!("`duration` should be an i64"),
        };
        gs.duration += (duration as f64) * weight;
        if error != 0 {
            gs.err_distribution.insert(duration as f64)
        } else {
            gs.ok_distribution.insert(duration as f64)
        }
    }
}

pub struct Aggregator {
    /// The key represents the timestamp (in nanoseconds) of the beginning of the time window (that lasts 10 seconds) on
    /// which the associated bucket will calculate statistics.
    buckets: BTreeMap<u64, Bucket>,

    /// The oldeest timestamp we will allow for the current time bucket.
    oldest_timestamp: u64,

    /// Env asociated with the Agent
    agent_env: Option<String>,

    /// Hostname associated with the Agent
    agent_hostname: Option<String>,

    /// Version associated with the Agent
    agent_version: Option<String>,

    /// TODO
    api_key: Option<Arc<str>>,

    /// TODO
    default_api_key: Arc<str>,
}

impl Aggregator {
    pub fn new(default_api_key: Arc<str>) -> Self {
        Self {
            buckets: BTreeMap::new(),
            oldest_timestamp: align_timestamp(Utc::now().timestamp_nanos() as u64),
            default_api_key,
            // We can't know the below fields until have received a trace event
            agent_env: None,
            agent_hostname: None,
            agent_version: None,
            api_key: None,
        }
    }

    /// TODO
    fn update_agent_properties(&mut self, partition_key: &PartitionKey) {
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
                self.api_key = Some(api_key.clone());
            }
        }
    }

    pub fn get_agent_env(&self) -> String {
        self.agent_env.clone().unwrap_or_default()
        //self.agent_env.as_ref().unwrap_or_default()
    }

    pub fn get_agent_hostname(&self) -> String {
        self.agent_hostname.clone().unwrap_or_default()
    }

    pub fn get_agent_version(&self) -> String {
        self.agent_version.clone().unwrap_or_default()
    }

    pub fn get_api_key(&self) -> Arc<str> {
        self.api_key
            .clone()
            .unwrap_or_else(|| Arc::clone(&self.default_api_key))
    }

    /// Iterates over a trace's constituting spans and upon matching conditions it updates statistics (mostly using the top level span).
    fn handle_trace(&mut self, partition_key: &PartitionKey, trace: &TraceEvent) {
        // Based on https://github.com/DataDog/datadog-agent/blob/cfa750c7412faa98e87a015f8ee670e5828bbe7f/pkg/trace/stats/concentrator.go#L148-L184

        let spans = match trace.get("spans") {
            Some(Value::Array(v)) => v.iter().filter_map(|s| s.as_object()).collect(),
            _ => vec![],
        };

        let weight = extract_weight_from_root_span(&spans);
        let payload_aggkey = PayloadAggregationKey {
            env: partition_key.env.clone().unwrap_or_default(),
            hostname: partition_key.hostname.clone().unwrap_or_default(),
            version: trace
                .get("app_version")
                .map(|v| v.to_string_lossy().into_owned())
                .unwrap_or_default(),
            container_id: trace
                .get("container_id")
                .map(|v| v.to_string_lossy().into_owned())
                .unwrap_or_default(),
        };
        let synthetics = trace
            .get("origin")
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
        span: &BTreeMap<String, Value>,
        weight: f64,
        is_top: bool,
        synthetics: bool,
        payload_aggkey: PayloadAggregationKey,
    ) {
        // Based on: https://github.com/DataDog/datadog-agent/blob/cfa750c7412faa98e87a015f8ee670e5828bbe7f/pkg/trace/stats/statsraw.go#L147-L182

        let aggkey = AggregationKey::new_aggregation_from_span(span, payload_aggkey, synthetics);

        let start = match span.get("start") {
            Some(Value::Timestamp(val)) => val.timestamp_nanos() as u64,
            _ => Utc::now().timestamp_nanos() as u64,
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
                // TODO change to debug
                info!("Created {} start_time bucket.", btime);
                self.buckets.insert(btime, b);
            }
        }
    }

    /// TODO
    fn export_buckets(
        &mut self,
        flush_cutoff_time: u64,
    ) -> BTreeMap<PayloadAggregationKey, Vec<ClientStatsBucket>> {
        let mut m = BTreeMap::<PayloadAggregationKey, Vec<ClientStatsBucket>>::new();

        self.buckets.retain(|&bucket_start, bucket| {
            let retain = bucket_start > flush_cutoff_time;

            if !retain {
                // TODO change back to debug
                info!("Flushing {} start_time bucket.", bucket_start);

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

    /// TODO
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

    /// Flushes the bucket cache.
    /// We cache and can compute stats only for the last `BUCKET_WINDOW_LEN * BUCKET_DURATION_NANOSECONDS` and after such time,
    /// buckets are then flushed. This only applies to past buckets. Stats buckets in the future are cached with no restriction.
    ///
    /// # Arguments
    ///
    /// * `force` - If true, all cached buckets are flushed.
    fn flush(&mut self, force: bool) -> Vec<ClientStatsPayload> {
        // Based on https://github.com/DataDog/datadog-agent/blob/cfa750c7412faa98e87a015f8ee670e5828bbe7f/pkg/trace/stats/concentrator.go#L38-L41
        // , and https://github.com/DataDog/datadog-agent/blob/cfa750c7412faa98e87a015f8ee670e5828bbe7f/pkg/trace/stats/concentrator.go#L195-L207

        let now = Utc::now().timestamp_nanos() as u64;

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
}

/// Returns the provided timestamp truncated to the bucket size.
/// This is the start time of the time bucket in which such timestamp falls.
const fn align_timestamp(start: u64) -> u64 {
    // Based on https://github.com/DataDog/datadog-agent/blob/cfa750c7412faa98e87a015f8ee670e5828bbe7f/pkg/trace/stats/concentrator.go#L232-L234
    start - (start % BUCKET_DURATION_NANOSECONDS)
}

/// Assumes that all metrics are all encoded as Value::Float.
/// Return the f64 of the specified key or None of key not present.
fn get_metric_value_float(span: &BTreeMap<String, Value>, key: &str) -> Option<f64> {
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
fn metric_value_is_1(span: &BTreeMap<String, Value>, key: &str) -> bool {
    match get_metric_value_float(span, key) {
        Some(f) => f == 1.0,
        None => false,
    }
}

/// Returns true if span is top-level.
fn has_top_level(span: &BTreeMap<String, Value>) -> bool {
    // Based on: https://github.com/DataDog/datadog-agent/blob/cfa750c7412faa98e87a015f8ee670e5828bbe7f/pkg/trace/traceutil/span.go#L28-L31

    metric_value_is_1(span, TOP_LEVEL_KEY)
}

/// Returns true if a span should be measured (i.e. it should get trace metrics calculated).
fn is_measured(span: &BTreeMap<String, Value>) -> bool {
    // Based on https://github.com/DataDog/datadog-agent/blob/cfa750c7412faa98e87a015f8ee670e5828bbe7f/pkg/trace/traceutil/span.go#L40-L43

    metric_value_is_1(span, MEASURED_KEY)
}

/// Returns true if the span is a partial snapshot.
/// These types of spans are partial images of long-running spans.
/// When incomplete, a partial snapshot has a metric _dd.partial_version which is a positive integer.
/// The metric usually increases each time a new version of the same span is sent by the tracer
fn is_partial_snapshot(span: &BTreeMap<String, Value>) -> bool {
    // Based on: https://github.com/DataDog/datadog-agent/blob/cfa750c7412faa98e87a015f8ee670e5828bbe7f/pkg/trace/traceutil/span.go#L49-L52

    match get_metric_value_float(span, PARTIAL_VERSION_KEY) {
        Some(f) => f >= 0.0,
        None => false,
    }
}

/// This extracts the relative weights from the top level span (i.e. the span that does not have a parent).
fn extract_weight_from_root_span(spans: &[&BTreeMap<String, Value>]) -> f64 {
    // Based on https://github.com/DataDog/datadog-agent/blob/cfa750c7412faa98e87a015f8ee670e5828bbe7f/pkg/trace/stats/weight.go#L17-L26.

    // TODO this logic likely has a bug(s) that need to be root caused. The root span is not reliably found and defaults to "1.0"
    // regularly for users even when sampling is disabled in the Agent.
    // GH issue to track that: https://github.com/vectordotdev/vector/issues/14859

    if spans.is_empty() {
        return 1.0;
    }

    let mut trace_id: Option<usize> = None;

    let mut parent_id_to_child_weight = BTreeMap::<i64, f64>::new();
    let mut span_ids = Vec::<i64>::new();
    for s in spans.iter() {
        // TODO these need to change to u64 when the following issue is fixed:
        // https://github.com/vectordotdev/vector/issues/14687
        let parent_id = match s.get("parent_id") {
            Some(Value::Integer(val)) => *val,
            None => 0,
            _ => panic!("`parent_id` should be an i64"),
        };
        let span_id = match s.get("span_id") {
            Some(Value::Integer(val)) => *val,
            None => 0,
            _ => panic!("`span_id` should be an i64"),
        };
        if trace_id.is_none() {
            trace_id = match s.get("trace_id") {
                Some(Value::Integer(v)) => Some(*v as usize),
                _ => panic!("`trace_id` should be an i64"),
            }
        }
        let weight = s
            .get("metrics")
            .and_then(|m| m.as_object())
            .map(|m| match m.get(SAMPLING_RATE_KEY) {
                Some(Value::Float(v)) => {
                    let sample_rate = v.into_inner();
                    if sample_rate <= 0.0 || sample_rate > 1.0 {
                        1.0
                    } else {
                        1.0 / sample_rate
                    }
                }
                _ => 1.0,
            })
            .unwrap_or(1.0);

        // found root
        if parent_id == 0 {
            return weight;
        }

        span_ids.push(span_id);

        parent_id_to_child_weight.insert(parent_id, weight);
    }

    // Remove all spans that have a parent
    span_ids.iter().for_each(|id| {
        parent_id_to_child_weight.remove(id);
    });

    // There should be only one value remaining, the weight from the root span
    if parent_id_to_child_weight.len() != 1 {
        // TODO remove the debug print and emit the Error event as outlined in
        // https://github.com/vectordotdev/vector/issues/14859
        debug!(
            "Didn't reliably find the root span for weight calculation of trace_id {:?}.",
            trace_id
        );
    }

    *parent_id_to_child_weight
        .values()
        .next()
        .unwrap_or_else(|| {
            // TODO remove the debug print and emit the Error event as outlined in
            // https://github.com/vectordotdev/vector/issues/14859
            debug!(
                "Root span was not found. Defaulting to weight of 1.0 for trace_id {:?}.",
                trace_id
            );
            &1.0
        })
}

/// TODO
///
/// # arguments
///
/// * `` -
pub(crate) fn compute_apm_stats(
    key: &PartitionKey,
    aggregator: Arc<Mutex<Aggregator>>,
    trace_events: &[TraceEvent],
) {
    let mut aggregator = aggregator.lock().unwrap();

    // store properties that are available only at runtime
    aggregator.update_agent_properties(&key);

    // process the incoming traces
    trace_events
        .iter()
        .for_each(|t| aggregator.handle_trace(key, t));
}

/// TODO
///
/// # arguments
///
/// * `` -
pub async fn flush_apm_stats_thread(
    mut tripwire: Receiver<()>,
    client: HttpClient,
    compression: Compression,
    endpoint_configuration: DatadogTracesEndpointConfiguration,
    aggregator: Arc<Mutex<Aggregator>>,
) {
    let sender = ApmStatsSender {
        client,
        compression,
        endpoint_configuration,
        aggregator,
    };

    // flush on the same interval as the stats buckets
    let mut interval =
        tokio::time::interval(std::time::Duration::from_nanos(BUCKET_DURATION_NANOSECONDS));

    // TODO change to debug
    info!("Starting APM stats flushing thread.");

    loop {
        tokio::select! {
            _ = interval.tick() => {
                sender.flush_apm_stats(false).await;
            },
            _ = tripwire.changed() =>  {
                // TODO change to debug
                info!("Ending APM stats flushing thread.");
                sender.flush_apm_stats(true).await;
                break;
            },
        }
    }
}

struct ApmStatsSender {
    pub client: HttpClient,
    pub compression: Compression,
    pub endpoint_configuration: DatadogTracesEndpointConfiguration,
    pub aggregator: Arc<Mutex<Aggregator>>,
}

impl ApmStatsSender {
    async fn flush_apm_stats(&self, force: bool) {
        // explicit scope to minimize duration that the Aggregator is locked.
        if let Some((payload, api_key)) = {
            let mut aggregator = self.aggregator.lock().unwrap();
            let client_stats_payloads = aggregator.flush(force);

            if client_stats_payloads.is_empty() {
                // no sense proceeding if no payloads to flush
                None
            } else {
                let payload = StatsPayload {
                    agent_hostname: aggregator.get_agent_hostname(),
                    agent_env: aggregator.get_agent_env(),
                    stats: client_stats_payloads,
                    agent_version: aggregator.get_agent_version(),
                    client_computed: false,
                };

                Some((payload, aggregator.get_api_key()))
            }
        } {
            if let Err(e) = self.compress_and_send(payload, api_key).await {
                // TODO emit an internal `Error` event here, probably
                error!(message = format!("Error while encoding APM stats payloads: {}", e));
            }
        }
    }

    async fn compress_and_send(
        &self,
        payload: StatsPayload,
        api_key: Arc<str>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let (metadata, compressed_payload) = self.build_apm_stats_request_data(api_key, payload)?;

        let request_metadata = RequestMetadata::new(0, 0, 0, 0, 0);
        let trace_api_request = build_request(
            (metadata, request_metadata),
            compressed_payload,
            &self.compression,
            &self.endpoint_configuration,
        );

        let http_request = trace_api_request
            .into_http_request()
            .context(BuildRequestSnafu)?;

        self.client.send(http_request).await?;

        Ok(())
    }

    fn build_apm_stats_request_data(
        &self,
        api_key: Arc<str>,
        payload: StatsPayload,
    ) -> Result<(DDTracesMetadata, Bytes), RequestBuilderError> {
        let encoded_payload =
            rmp_serde::to_vec_named(&payload).map_err(|e| RequestBuilderError::FailedToEncode {
                message: "APM stats encoding failed.",
                reason: e.to_string(),
                dropped_events: 0,
            })?;
        let uncompressed_size = encoded_payload.len();
        let metadata = DDTracesMetadata {
            api_key,
            endpoint: DatadogTracesEndpoint::APMStats,
            finalizers: EventFinalizers::default(),
            uncompressed_size,
            content_type: "application/msgpack".to_string(),
        };

        let mut compressor = Compressor::from(self.compression);
        match compressor.write_all(&encoded_payload) {
            Ok(()) => {
                let bytes = compressor.into_inner().freeze();

                Ok((metadata, bytes))
            }
            Err(e) => Err(RequestBuilderError::FailedToEncode {
                message: "APM stats payload compression failed.",
                reason: e.to_string(),
                dropped_events: 0,
            }),
        }
    }
}

// On the agent side APM Stats payload are encoded into the messagepack format using this
// go code https://github.com/DataDog/datadog-agent/blob/b5bed4d/pkg/trace/pb/stats_gen.go.
// Note that this code is generated from code itself generate from this .proto file
// https://github.com/DataDog/datadog-agent/blob/dc2f202/pkg/trace/pb/stats.proto.
// All the subsequent struct are dedicated to be used with rmp_serde and the fields names
// exactly match the ones of the go code.
#[derive(Debug, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "PascalCase")]
pub(crate) struct StatsPayload {
    pub(crate) agent_hostname: String,
    pub(crate) agent_env: String,
    pub(crate) stats: Vec<ClientStatsPayload>,
    pub(crate) agent_version: String,
    pub(crate) client_computed: bool,
}

#[derive(Debug, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "PascalCase")]
pub(crate) struct ClientStatsPayload {
    pub(crate) hostname: String,
    pub(crate) env: String,
    pub(crate) version: String,
    pub(crate) stats: Vec<ClientStatsBucket>,
    pub(crate) lang: String,
    pub(crate) tracer_version: String,
    #[serde(rename = "RuntimeID")]
    pub(crate) runtime_id: String,
    pub(crate) sequence: u64,
    pub(crate) agent_aggregation: String,
    pub(crate) service: String,
    #[serde(rename = "ContainerID")]
    pub(crate) container_id: String,
    pub(crate) tags: Vec<String>,
}

#[derive(Debug, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "PascalCase")]
pub(crate) struct ClientStatsBucket {
    pub(crate) start: u64,
    pub(crate) duration: u64,
    pub(crate) stats: Vec<ClientGroupedStats>,
    pub(crate) agent_time_shift: i64,
}

#[derive(Debug, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "PascalCase")]
pub(crate) struct ClientGroupedStats {
    pub(crate) service: String,
    pub(crate) name: String,
    pub(crate) resource: String,
    #[serde(rename = "HTTPStatusCode")]
    pub(crate) http_status_code: u32,
    pub(crate) r#type: String,
    #[serde(rename = "DBType")]
    pub(crate) db_type: String,
    pub(crate) hits: u64,
    pub(crate) errors: u64,
    pub(crate) duration: u64,
    #[serde(with = "serde_bytes")]
    pub(crate) ok_summary: Vec<u8>,
    #[serde(with = "serde_bytes")]
    pub(crate) error_summary: Vec<u8>,
    pub(crate) synthetics: bool,
    pub(crate) top_level_hits: u64,
}
