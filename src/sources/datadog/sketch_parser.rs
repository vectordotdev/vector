use std::{collections::BTreeMap, sync::Arc};

use bytes::Bytes;
use chrono::{TimeZone, Utc};
use prost::Message;
use vector_core::{config::log_schema, metrics::AgentDDSketch};

use crate::{
    event::{metric::MetricValue, Event, Metric, MetricKind},
    Result,
};

mod dd_proto {
    include!(concat!(env!("OUT_DIR"), "/datadog.agentpayload.rs"));
}

use dd_proto::SketchPayload;

pub(crate) fn decode_ddsketch(frame: Bytes, api_key: &Option<Arc<str>>) -> Result<Vec<Event>> {
    let payload = SketchPayload::decode(frame)?;
    // payload.metadata is always empty for payload coming from dd agents
    Ok(payload
        .sketches
        .into_iter()
        .flat_map(|sketch_series| {
            // sketch_series.distributions is also always empty from payload coming from dd agents
            let mut tags: BTreeMap<String, String> = sketch_series
                .tags
                .iter()
                .map(|tag| {
                    let kv = tag.split_once(":").unwrap_or((tag, ""));
                    (kv.0.trim().into(), kv.1.trim().into())
                })
                .collect();

            tags.insert(
                log_schema().host_key().to_string(),
                sketch_series.host.clone(),
            );
            sketch_series.dogsketches.into_iter().map(move |sketch| {
                let k: Vec<i16> = sketch.k.iter().map(|k| *k as i16).collect();
                let n: Vec<u16> = sketch.n.iter().map(|n| *n as u16).collect();
                let val = MetricValue::from(
                    AgentDDSketch::from_raw(
                        sketch.cnt as u32,
                        sketch.min,
                        sketch.max,
                        sketch.sum,
                        sketch.avg,
                        &k,
                        &n,
                    )
                    .unwrap_or_else(AgentDDSketch::with_agent_defaults),
                );
                let mut metric =
                    Metric::new(sketch_series.metric.clone(), MetricKind::Incremental, val)
                        .with_tags(Some(tags.clone()))
                        .with_timestamp(Some(Utc.timestamp(sketch.ts, 0)));
                if let Some(k) = &api_key {
                    metric
                        .metadata_mut()
                        .set_datadog_api_key(Some(Arc::clone(k)));
                }
                metric.into()
            })
        })
        .collect())
}
