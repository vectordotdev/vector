use async_graphql::{Enum, Object};
use chrono::{DateTime, Utc};
use serde_json::Value;
use vector_lib::encode_logfmt;

use super::EventEncodingType;
use crate::{
    event::{self, KeyString},
    topology::TapOutput,
};

#[derive(Debug, Clone)]
pub struct Metric {
    output: TapOutput,
    event: event::Metric,
}

impl Metric {
    pub const fn new(output: TapOutput, event: event::Metric) -> Self {
        Self { output, event }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Enum)]
enum MetricKind {
    /// Incremental metrics update previous values
    Incremental,
    /// Absolute metrics set the reference value for future updates
    Absolute,
}

impl From<event::MetricKind> for MetricKind {
    fn from(kind: event::MetricKind) -> Self {
        match kind {
            event::MetricKind::Incremental => Self::Incremental,
            event::MetricKind::Absolute => Self::Absolute,
        }
    }
}

struct MetricTag {
    key: String,
    value: String,
}

#[Object]
impl MetricTag {
    /// Metric tag key
    async fn key(&self) -> &str {
        self.key.as_ref()
    }

    /// Metric tag value
    async fn value(&self) -> &str {
        self.value.as_ref()
    }
}

#[Object]
/// Metric event with fields for querying metric data
impl Metric {
    /// Id of the component associated with the metric event
    async fn component_id(&self) -> &str {
        self.output.output_id.component.id()
    }

    /// Type of component associated with the metric event
    async fn component_type(&self) -> &str {
        self.output.component_type.as_ref()
    }

    /// Kind of component associated with the metric event
    async fn component_kind(&self) -> &str {
        self.output.component_kind
    }

    /// Metric timestamp
    async fn timestamp(&self) -> Option<&DateTime<Utc>> {
        self.event.data().timestamp()
    }

    /// Metric name
    async fn name(&self) -> &str {
        self.event.name()
    }

    /// Metric namespace
    async fn namespace(&self) -> Option<&str> {
        self.event.namespace()
    }

    /// Metric kind
    async fn kind(&self) -> MetricKind {
        self.event.kind().into()
    }

    /// Metric type
    async fn value_type(&self) -> &str {
        self.event.value().as_name()
    }

    /// Metric value in human readable form
    async fn value(&self) -> String {
        self.event.value().to_string()
    }

    /// Metric tags
    async fn tags(&self) -> Option<Vec<MetricTag>> {
        self.event.tags().map(|tags| {
            tags.iter_single()
                .map(|(key, value)| MetricTag {
                    key: key.to_owned(),
                    value: value.to_owned(),
                })
                .collect()
        })
    }

    /// Metric event as an encoded string format
    async fn string(&self, encoding: EventEncodingType) -> String {
        match encoding {
            EventEncodingType::Json => serde_json::to_string(&self.event)
                .expect("JSON serialization of metric event failed. Please report."),
            EventEncodingType::Yaml => serde_yaml::to_string(&self.event)
                .expect("YAML serialization of metric event failed. Please report."),
            EventEncodingType::Logfmt => {
                let json = serde_json::to_value(&self.event)
                    .expect("logfmt serialization of metric event failed: conversion to serde Value failed. Please report.");
                match json {
                    Value::Object(map) => encode_logfmt::encode_map(
                        &map.into_iter().map(|(k,v)| (KeyString::from(k), v)).collect(),
                    )
                    .expect("logfmt serialization of metric event failed. Please report."),
                    _ => panic!("logfmt serialization of metric event failed: metric converted to unexpected serde Value. Please report."),
                }
            }
        }
    }
}
