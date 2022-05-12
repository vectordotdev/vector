use crate::{event::TraceEvent, sinks::datadog::traces::sink::PartitionKey};
mod dd_proto {
    include!(concat!(env!("OUT_DIR"), "/dd_trace.rs"));
}

pub(crate) fn compute_apm_stats(
    _key: &PartitionKey,
    _events: &[TraceEvent],
) -> dd_proto::StatsPayload {
    dd_proto::StatsPayload {
        agent_hostname: "dummy".to_string(),
        agent_env: "dummy".to_string(),
        stats: vec![],
        agent_version: "dummy".to_string(),
        client_computed: false,
    }
}
