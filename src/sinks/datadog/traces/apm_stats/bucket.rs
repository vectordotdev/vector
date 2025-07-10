use std::collections::BTreeMap;

use prost::Message;

use super::{
    aggregation::{AggregationKey, PayloadAggregationKey},
    ddsketch_full, ClientGroupedStats, ClientStatsBucket,
};
use crate::{event::ObjectMap, event::Value, metrics::AgentDDSketch};

pub(crate) struct GroupedStats {
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

    // zeroes depicts the number of values that fell around zero based on the sketch local accuracy
    // positives and negatives stores are respectively storing positive and negative values using the
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
        .zip(bin_map.counts)
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
pub(crate) struct Bucket {
    pub(crate) start: u64,
    pub(crate) duration: u64,
    pub(crate) data: BTreeMap<AggregationKey, GroupedStats>,
}

impl Bucket {
    pub(crate) fn export(&self) -> BTreeMap<PayloadAggregationKey, ClientStatsBucket> {
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

    pub(crate) fn add(
        &mut self,
        span: &ObjectMap,
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
    fn update(span: &ObjectMap, weight: f64, is_top: bool, gs: &mut GroupedStats) {
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
