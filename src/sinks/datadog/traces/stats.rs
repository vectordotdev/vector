use crate::{event::TraceEvent, sinks::datadog::traces::sink::PartitionKey};
use std::collections::BTreeMap;
use crate::event::Value;
mod dd_proto {
    include!(concat!(env!("OUT_DIR"), "/dd_trace.rs"));
}

const TOP_LEVEL_KEY: &str = "_dd.top_level";

struct Aggregator {}

impl Aggregator {
    fn new() -> Self {
        Self {}
    }

    fn handle_trace(&self, _trace: &TraceEvent) {}

    fn handle_span(&self, _span: &BTreeMap<String, Value>) {}

    fn get_client_stats_payload(&self) -> Vec<dd_proto::ClientStatsPayload> {
        vec![]
    }
}
pub(crate) fn compute_apm_stats(
    key: &PartitionKey,
    traces: &[TraceEvent],
) -> dd_proto::StatsPayload {
    let aggregator = Aggregator::new();
    traces.iter().for_each(|t| aggregator.handle_trace(t));

    dd_proto::StatsPayload {
        agent_hostname: key.hostname.clone().unwrap_or_else(|| "".to_string()),
        agent_env: key.env.clone().unwrap_or_else(|| "".to_string()),
        stats: aggregator.get_client_stats_payload(),
        agent_version: key.agent_version.clone().unwrap_or_else(|| "".to_string()),
        client_computed: false,
    }
}
