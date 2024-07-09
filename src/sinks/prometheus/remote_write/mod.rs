//! The Prometheus Remote Write sink.
//! Contains the [`VectorSink`] instance that is responsible
//! for taking a stream of [`Event`] and forwarding
//! them to a server via the [Prometheus Remote Write protocol][remote_write].
//!
//! [remote_write]: https://prometheus.io/docs/concepts/remote_write_spec/

use vector_lib::event::Metric;

use crate::sinks::{
    prelude::*,
    util::buffer::metrics::{MetricNormalize, MetricSet},
};

mod config;
mod request_builder;
mod service;
mod sink;

#[cfg(test)]
mod tests;

#[cfg(all(test, feature = "prometheus-integration-tests"))]
mod integration_tests;

#[cfg(all(test, feature = "sources-prometheus-remote-write"))]
pub use config::RemoteWriteConfig;

#[cfg(feature = "aws-core")]
#[derive(Debug, snafu::Snafu)]
enum Errors {
    #[snafu(display("aws.region required when AWS authentication is in use"))]
    AwsRegionRequired,
}

#[derive(Clone, Eq, Hash, PartialEq)]
struct PartitionKey {
    tenant_id: Option<String>,
}

#[derive(Default)]
pub struct PrometheusMetricNormalize;

impl MetricNormalize for PrometheusMetricNormalize {
    fn normalize(&mut self, state: &mut MetricSet, metric: Metric) -> Option<Metric> {
        state.make_absolute(metric)
    }
}
