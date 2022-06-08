use std::collections::BTreeMap;

use chrono::Utc;
use prost::Message;
use serde::{Deserialize, Serialize};
use serde_bytes;

use super::{ddsketch_full, sink::PartitionKey};
use crate::{
    event::{TraceEvent, Value},
    metrics::AgentDDSketch,
};

const TOP_LEVEL_KEY: &str = "_top_level";
const SAMPLING_RATE_KEY: &str = "_sample_rate";
const MEASURED_KEY: &str = "_dd.measured";
const PARTIAL_VERSION_KEY: &str = "_dd.partial_version";

#[derive(PartialEq, Eq, PartialOrd, Ord)]
struct AggregationKey {
    payload_key: PayloadAggregationKey,
    bucket_key: BucketAggregationKey,
}

impl AggregationKey {
    fn new_aggregation_from_span(
        span: &BTreeMap<String, Value>,
        payload_key: PayloadAggregationKey,
    ) -> Self {
        AggregationKey {
            payload_key: payload_key.with_span_context(span),
            bucket_key: BucketAggregationKey {
                service: span
                    .get("service")
                    .map(|v| v.to_string_lossy())
                    .unwrap_or_default(),
                name: span
                    .get("name")
                    .map(|v| v.to_string_lossy())
                    .unwrap_or_default(),
                resource: span
                    .get("resource")
                    .map(|v| v.to_string_lossy())
                    .unwrap_or_default(),
                ty: span
                    .get("type")
                    .map(|v| v.to_string_lossy())
                    .unwrap_or_default(),
                status_code: 0,
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
                .map(|s| s.to_string_lossy())
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
            synthetics: false,
            top_level_hits: self.top_level_hits.round() as u64,
        }
    }
}

/// Convert agent sketch variant to ./proto/dd_sketch_full.proto
fn encode_sketch(agent_sketch: &AgentDDSketch) -> Vec<u8> {
    let index_mapping = ddsketch_full::IndexMapping {
        gamma: agent_sketch.gamma(),
        index_offset: agent_sketch.bin_index_offset() as f64,
        interpolation: ddsketch_full::index_mapping::Interpolation::None as i32,
    };

    let (positives, negatives, zeroes) = convert_stores(agent_sketch);
    let positives_store = ddsketch_full::Store {
        bin_counts: positives,
        contiguous_bin_counts: Vec::new(),
        contiguous_bin_index_offset: 0,
    };
    let negatives_store = ddsketch_full::Store {
        bin_counts: negatives,
        contiguous_bin_counts: Vec::new(),
        contiguous_bin_index_offset: 0,
    };
    let s = ddsketch_full::DdSketch {
        mapping: Some(index_mapping),
        positive_values: Some(positives_store),
        negative_values: Some(negatives_store),
        zero_count: zeroes,
    };
    s.encode_to_vec()
}

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

    fn update(span: &BTreeMap<String, Value>, weight: f64, is_top: bool, gs: &mut GroupedStats) {
        is_top.then(|| {
            gs.top_level_hits += weight;
        });
        gs.hits += weight;
        let error = match span.get("error") {
            Some(Value::Integer(val)) => *val,
            _ => 0,
        };
        if error != 0 {
            gs.errors += weight;
        }
        let duration = match span.get("duration") {
            Some(Value::Integer(val)) => *val,
            _ => 0,
        };
        gs.duration += (duration as f64) * weight;
        if error != 0 {
            gs.err_distribution.insert(duration as f64)
        } else {
            gs.ok_distribution.insert(duration as f64)
        }
    }
}

struct Aggregator {
    buckets: BTreeMap<u64, Bucket>,
}

impl Aggregator {
    fn new() -> Self {
        Self {
            buckets: BTreeMap::new(),
        }
    }

    fn handle_trace(&mut self, partition_key: &PartitionKey, trace: &TraceEvent) {
        let spans = match trace.get("spans") {
            Some(Value::Array(v)) => v.iter().filter_map(|s| s.as_object()).collect(),
            _ => vec![],
        };

        let weigth = extract_weigth_from_root_span(&spans);
        let payload_aggkey = PayloadAggregationKey {
            env: partition_key.env.clone().unwrap_or_default(),
            hostname: partition_key.hostname.clone().unwrap_or_default(),
            version: trace
                .get("app_version")
                .map(|v| v.to_string_lossy())
                .unwrap_or_default(),
            container_id: trace
                .get("container_id")
                .map(|v| v.to_string_lossy())
                .unwrap_or_default(),
        };

        spans.iter().for_each(|span| {
            let is_top = metric_flag(span, TOP_LEVEL_KEY);
            if !(is_top
                || metric_flag(span, MEASURED_KEY)
                || metric_flag(span, PARTIAL_VERSION_KEY))
            {
                return;
            }
            self.handle_span(span, weigth, is_top, payload_aggkey.clone());
        });
    }

    fn handle_span(
        &mut self,
        span: &BTreeMap<String, Value>,
        weight: f64,
        is_top: bool,
        payload_aggkey: PayloadAggregationKey,
    ) {
        let aggkey = AggregationKey::new_aggregation_from_span(span, payload_aggkey);
        let start = match span.get("start") {
            Some(Value::Timestamp(val)) => val.timestamp_nanos(),
            _ => Utc::now().timestamp_nanos(),
        };
        // 10 seconds bucket
        let btime = (start - (start % 10_000_000_000)) as u64;
        match self.buckets.get_mut(&btime) {
            Some(b) => {
                b.add(span, weight, is_top, aggkey);
            }
            None => {
                let mut b = Bucket {
                    start: btime,
                    duration: 10_000_000_000, // 10 secs in nanosecs
                    data: BTreeMap::new(),
                };
                b.add(span, weight, is_top, aggkey);
                self.buckets.insert(btime, b);
            }
        }
    }

    fn get_client_stats_payload(&self) -> Vec<ClientStatsPayload> {
        let client_stats_buckets = self.export_buckets();

        client_stats_buckets
            .into_iter()
            .map(|(payload_aggkey, csb)| {
                ClientStatsPayload {
                    env: payload_aggkey.env,
                    hostname: payload_aggkey.hostname,
                    container_id: payload_aggkey.container_id,
                    version: payload_aggkey.version,
                    stats: csb,
                    service: "".to_string(), // not set by the trace-agent, already set in stats
                    agent_aggregation: "".to_string(), // not set by the trace-agent
                    sequence: 0,             // not set by the trace-agent
                    runtime_id: "".to_string(), // not set by the trace-agent
                    lang: "".to_string(),    //not set by the trace-agent
                    tracer_version: "".to_string(), // not set by the trace-agent
                    tags: vec![],            // not set by the trace-agent
                }
            })
            .collect::<Vec<ClientStatsPayload>>()
    }

    fn export_buckets(&self) -> BTreeMap<PayloadAggregationKey, Vec<ClientStatsBucket>> {
        let mut m = BTreeMap::<PayloadAggregationKey, Vec<ClientStatsBucket>>::new();
        self.buckets.values().for_each(|b| {
            b.export().into_iter().for_each(|(payload_key, csb)| {
                match m.get_mut(&payload_key) {
                    None => {
                        m.insert(payload_key.clone(), vec![csb]);
                    }
                    Some(s) => {
                        s.push(csb);
                    }
                };
            })
        });
        m
    }
}

fn metric_flag(span: &BTreeMap<String, Value>, key: &str) -> bool {
    span.get("metrics")
        .and_then(|m| m.as_object())
        .map(|m| match m.get(key) {
            Some(Value::Float(f)) => f.into_inner().signum() == 1.0,
            _ => false,
        })
        .unwrap_or(false)
}

fn extract_weigth_from_root_span(spans: &[&BTreeMap<String, Value>]) -> f64 {
    if spans.is_empty() {
        return 1.0;
    }
    let mut parent_id_to_child_weigth = BTreeMap::<i64, f64>::new();
    let mut span_ids = Vec::<i64>::new();
    for s in spans.iter() {
        let parent_id = match s.get("parent_id") {
            Some(Value::Integer(val)) => *val,
            _ => 0,
        };
        let span_id = match s.get("span_id") {
            Some(Value::Integer(val)) => *val,
            _ => 0,
        };
        let sample_rate = s
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
        if parent_id == 0 {
            return sample_rate;
        }
        span_ids.push(span_id);
        parent_id_to_child_weigth.insert(parent_id, sample_rate);
    }
    // We remove all span that do have a parent
    span_ids.iter().for_each(|id| {
        parent_id_to_child_weigth.remove(id);
    });
    // There should be only one value remaining, the weigth from the root span
    if parent_id_to_child_weigth.len() != 1 {
        debug!("Didn't reliably find the root span.");
    }

    *parent_id_to_child_weigth
        .values()
        .next()
        .unwrap_or_else(|| {
            debug!("Root span was not found");
            &1.0
        })
}

pub(crate) fn compute_apm_stats(key: &PartitionKey, traces: &[TraceEvent]) -> StatsPayload {
    let mut aggregator = Aggregator::new();
    traces.iter().for_each(|t| aggregator.handle_trace(key, t));
    StatsPayload {
        agent_hostname: key.hostname.clone().unwrap_or_else(|| "".to_string()),
        agent_env: key.env.clone().unwrap_or_else(|| "".to_string()),
        stats: aggregator.get_client_stats_payload(),
        agent_version: key.agent_version.clone().unwrap_or_else(|| "".to_string()),
        client_computed: false,
    }
}

// Stats struct come from .proto and field name are set to match go
// See this https://github.com/DataDog/datadog-agent/blob/b5bed4d/pkg/trace/pb/stats_gen.go for the reference go code
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
