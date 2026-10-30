//! Cardinality of fully decoded OTLP requests, independent of their envelopes.
use bytes::Bytes;
use prost::Message;
use vector_lib::opentelemetry::proto::{
    collector::{
        logs::v1::ExportLogsServiceRequest, metrics::v1::ExportMetricsServiceRequest,
        trace::v1::ExportTraceServiceRequest,
    },
    metrics::v1::metric::Data,
};

use super::config::{LOGS, METRICS, TRACES};

pub(super) fn traces(request: &ExportTraceServiceRequest) -> bool {
    request.resource_spans.iter().all(|resource| {
        resource
            .scope_spans
            .iter()
            .all(|scope| scope.spans.is_empty())
    })
}

pub(super) fn logs(request: &ExportLogsServiceRequest) -> bool {
    request.resource_logs.iter().all(|resource| {
        resource
            .scope_logs
            .iter()
            .all(|scope| scope.log_records.is_empty())
    })
}

pub(super) fn metrics(request: &ExportMetricsServiceRequest) -> bool {
    request.resource_metrics.iter().all(|resource| {
        resource.scope_metrics.iter().all(|scope| {
            scope
                .metrics
                .iter()
                .all(|metric| match metric.data.as_ref() {
                    None => true,
                    Some(Data::Gauge(data)) => data.data_points.is_empty(),
                    Some(Data::Sum(data)) => data.data_points.is_empty(),
                    Some(Data::Histogram(data)) => data.data_points.is_empty(),
                    Some(Data::ExponentialHistogram(data)) => data.data_points.is_empty(),
                    Some(Data::Summary(data)) => data.data_points.is_empty(),
                })
        })
    })
}

pub(super) fn protobuf(signal: &str, body: Bytes) -> Result<bool, prost::DecodeError> {
    // The endpoint supplies the signal; empty bytes cannot discriminate a generic OTLP codec.
    match signal {
        TRACES => ExportTraceServiceRequest::decode(body).map(|request| traces(&request)),
        LOGS => ExportLogsServiceRequest::decode(body).map(|request| logs(&request)),
        METRICS => ExportMetricsServiceRequest::decode(body).map(|request| metrics(&request)),
        _ => unreachable!("only OTLP signal endpoints call this helper"),
    }
}
