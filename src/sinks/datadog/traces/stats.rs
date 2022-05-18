use std::{collections::BTreeMap, time};

use chrono::Utc;
use prost::Message;

use crate::{
    event::{TraceEvent, Value},
    metrics::AgentDDSketch,
    sinks::datadog::traces::sink::PartitionKey,
};
mod dd_proto {
    include!(concat!(env!("OUT_DIR"), "/dd_trace.rs"));
}

mod ddsketch_full {
    include!(concat!(env!("OUT_DIR"), "/ddsketch_full.rs"));
}

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
            payload_key: payload_key.clone(),
            bucket_key: BucketAggregationKey {
                service: span
                    .get("service")
                    .map(|v| v.to_string_lossy())
                    .unwrap_or_else(|| "".into()),
                name: span
                    .get("name")
                    .map(|v| v.to_string_lossy())
                    .unwrap_or_else(|| "".into()),
                resource: span
                    .get("resource")
                    .map(|v| v.to_string_lossy())
                    .unwrap_or_else(|| "".into()),
                ty: span
                    .get("type")
                    .map(|v| v.to_string_lossy())
                    .unwrap_or_else(|| "".into()),
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

    fn export(&self, key: &AggregationKey) -> dd_proto::ClientGroupedStats {
        dd_proto::ClientGroupedStats {
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
            error_summary: encode_sketch(&self.ok_distribution),
            synthetics: false,
            top_level_hits: self.top_level_hits.round() as u64,
        }
    }
}

/// Convert agent sketch variant to ./proto/dd_sketch_full.proto
/// see this for conversion https://github.com/DataDog/datadog-agent/pull/11146
fn encode_sketch(_agent_sketch: &AgentDDSketch) -> Vec<u8> {
    let s = ddsketch_full::DdSketch {
        mapping: None,
        positive_values: None,
        negative_values: None,
        zero_count: 0.0,
    };
    s.encode_to_vec()
}
struct Bucket {
    start: u64,
    duration: u64,
    data: BTreeMap<AggregationKey, GroupedStats>,
}

impl Bucket {
    fn export(&self) -> BTreeMap<PayloadAggregationKey, dd_proto::ClientStatsBucket> {
        let mut m = BTreeMap::<PayloadAggregationKey, dd_proto::ClientStatsBucket>::new();
        self.data.iter().for_each(|(k, v)| {
            let b = v.export(k);
            match m.get_mut(&k.payload_key) {
                None => {
                    let sb = dd_proto::ClientStatsBucket {
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
        let duration = match span.get("duration") {
            Some(Value::Integer(val)) => *val,
            _ => 0,
        };
        let error = match span.get("error") {
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
    bucket: Bucket,
}

impl Aggregator {
    fn new() -> Self {
        Self {
            bucket: Bucket {
                start: (Utc::now() - chrono::Duration::seconds(10)).timestamp_nanos() as u64,
                duration: time::Duration::from_secs(10).as_nanos() as u64, // This is fixed now, we assume static 10 sec windows
                data: BTreeMap::new(),
            },
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
                .clone()
                .map(|v| v.to_string_lossy())
                .unwrap_or_default(),
            container_id: trace
                .get("container_id")
                .clone()
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
        self.bucket.add(span, weight, is_top, aggkey);
    }

    fn get_client_stats_payload(&self) -> Vec<dd_proto::ClientStatsPayload> {
        let client_stats_buckets = self.bucket.export();

        client_stats_buckets
            .into_iter()
            .map(|(payload_aggkey, csb)| {
                dd_proto::ClientStatsPayload {
                    env: payload_aggkey.env,
                    hostname: payload_aggkey.hostname,
                    container_id: payload_aggkey.container_id,
                    version: payload_aggkey.version,
                    stats: vec![csb],
                    service: "".to_string(),           // already set in stats
                    agent_aggregation: "".to_string(), // unsure about what is does
                    sequence: 0,                       // not set by the agent
                    runtime_id: "".to_string(), // TODO: bring that value from traces if relevant (TBC)
                    lang: "".to_string(), // TODO: bring that value from traces if relevant (TBC)
                    tracer_version: "".to_string(), // TODO: bring that value from traces if relevant (TBC)
                    tags: vec![],                   // empty on purpose
                }
            })
            .collect::<Vec<dd_proto::ClientStatsPayload>>()
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

fn extract_weigth_from_root_span(spans: &Vec<&BTreeMap<String, Value>>) -> f64 {
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
    for v in parent_id_to_child_weigth.values() {
        return *v;
    }
    debug!("Root span was not found");
    1.0
}

pub(crate) fn compute_apm_stats(
    key: &PartitionKey,
    traces: &[TraceEvent],
) -> dd_proto::StatsPayload {
    let mut aggregator = Aggregator::new();
    traces.iter().for_each(|t| aggregator.handle_trace(key, t));
    dd_proto::StatsPayload {
        agent_hostname: key.hostname.clone().unwrap_or_else(|| "".to_string()),
        agent_env: key.env.clone().unwrap_or_else(|| "".to_string()),
        stats: aggregator.get_client_stats_payload(),
        agent_version: key.agent_version.clone().unwrap_or_else(|| "".to_string()),
        client_computed: false,
    }
}
