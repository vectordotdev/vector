use async_graphql::Object;
use vector_lib::config::ComponentKey;

use super::{by_component_key, sum_metrics, SentEventsTotal};
use crate::event::Metric;

#[derive(Debug, Clone)]
pub struct Output {
    output_id: String,
    sent_events_total: Option<Metric>,
}

impl Output {
    pub const fn new(output_id: String, sent_events_total: Option<Metric>) -> Self {
        Self {
            output_id,
            sent_events_total,
        }
    }
}

#[Object]
impl Output {
    /// Id of the output stream
    pub async fn output_id(&self) -> &str {
        self.output_id.as_ref()
    }

    /// Total sent events for the current output stream
    pub async fn sent_events_total(&self) -> Option<SentEventsTotal> {
        self.sent_events_total
            .as_ref()
            .map(|metric| SentEventsTotal::new(metric.clone()))
    }
}

#[derive(Debug, Clone)]
pub struct OutputThroughput {
    output_id: String,
    throughput: i64,
}

impl OutputThroughput {
    pub const fn new(output_id: String, throughput: i64) -> Self {
        Self {
            output_id,
            throughput,
        }
    }
}

#[Object]
impl OutputThroughput {
    /// Id of the output stream
    pub async fn output_id(&self) -> &str {
        self.output_id.as_ref()
    }

    /// Throughput for the output stream
    pub async fn throughput(&self) -> i64 {
        self.throughput
    }
}

pub fn outputs_by_component_key(component_key: &ComponentKey, outputs: &[String]) -> Vec<Output> {
    let metrics = by_component_key(component_key)
        .into_iter()
        .filter(|m| m.name() == "component_sent_events_total")
        .collect::<Vec<_>>();

    outputs
        .iter()
        .map(|output| {
            Output::new(
                output.clone(),
                filter_output_metric(&metrics, output.as_ref()),
            )
        })
        .collect::<Vec<_>>()
}

pub fn filter_output_metric(metrics: &[Metric], output_name: &str) -> Option<Metric> {
    sum_metrics(
        metrics
            .iter()
            .filter(|m| m.tag_matches("output", output_name)),
    )
}
