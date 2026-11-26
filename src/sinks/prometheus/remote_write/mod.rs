//! The Prometheus Remote Write sink.
//! Contains the [`VectorSink`] instance that is responsible
//! for taking a stream of [`Event`] and forwarding
//! them to a server via the [Prometheus Remote Write protocol][remote_write].
//!
//! [remote_write]: https://prometheus.io/docs/concepts/remote_write_spec/

#[cfg(test)]
use vector_lib::event::Metric;

#[cfg(test)]
use crate::sinks::prelude::*;
use crate::sinks::util::Compression;

mod config;
mod request_builder;
mod service;
mod sink;

#[cfg(test)]
mod tests;

#[cfg(all(test, feature = "prometheus-integration-tests"))]
mod integration_tests;

#[cfg(all(test, feature = "sources-prometheus_remote_write"))]
pub(crate) use config::RemoteWriteConfig;

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

mod normalize;
pub use self::normalize::PrometheusMetricNormalize;
