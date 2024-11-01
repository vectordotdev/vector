//! APM stats
//!
//! This module contains the logic for computing APM statistics based on the incoming trace
//! events this sink receives. It is modelled closely to the trace-agent component of the
//! Datadog Agent, and sends out StatsPayload packets formatted and Aggregated by the same
//! algorithm, at ten second intervals, independently of the sink's trace payloads.

use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use vector_lib::event::TraceEvent;

pub use self::aggregation::Aggregator;
pub use self::flusher::flush_apm_stats_thread;

pub(crate) use super::config::{DatadogTracesEndpoint, DatadogTracesEndpointConfiguration};
pub(crate) use super::request_builder::{build_request, DDTracesMetadata, RequestBuilderError};
pub(crate) use super::sink::PartitionKey;

mod aggregation;
mod bucket;
mod flusher;
mod weight;

#[cfg(all(test, feature = "datadog-traces-integration-tests"))]
mod integration_tests;

/// The duration of time in nanoseconds that a bucket covers.
pub(crate) const BUCKET_DURATION_NANOSECONDS: u64 = 10_000_000_000;

#[allow(warnings, clippy::pedantic, clippy::nursery)]
pub(crate) mod ddsketch_full {
    include!(concat!(env!("OUT_DIR"), "/ddsketch_full.rs"));
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

/// Computes APM stats from the incoming trace events.
///
/// # arguments
///
/// * `key`           - PartitionKey associated with this set of trace events.
/// * `aggregator`    - Aggregator to use in computing and caching APM stats buckets.
/// * `trace_events`  - Newly received trace events to process.
pub(crate) fn compute_apm_stats(
    key: &PartitionKey,
    aggregator: Arc<Mutex<Aggregator>>,
    trace_events: &[TraceEvent],
) {
    let mut aggregator = aggregator.lock().unwrap();

    // store properties that are available only at runtime
    aggregator.update_agent_properties(key);

    // process the incoming traces
    trace_events
        .iter()
        .for_each(|t| aggregator.handle_trace(key, t));
}
