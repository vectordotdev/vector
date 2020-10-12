use crate::Event;
use futures::{
    compat::{Compat, Future01CompatExt},
    future::BoxFuture,
    StreamExt, TryFutureExt,
};
use futures01::Stream;
use snafu::Snafu;
use std::fmt;

pub mod util;

#[cfg(feature = "sinks-aws_cloudwatch_logs")]
pub mod aws_cloudwatch_logs;
#[cfg(feature = "sinks-aws_cloudwatch_metrics")]
pub mod aws_cloudwatch_metrics;
#[cfg(feature = "sinks-aws_kinesis_firehose")]
pub mod aws_kinesis_firehose;
#[cfg(feature = "sinks-aws_kinesis_streams")]
pub mod aws_kinesis_streams;
#[cfg(feature = "sinks-aws_s3")]
pub mod aws_s3;
#[cfg(feature = "sinks-azure_monitor_logs")]
pub mod azure_monitor_logs;
#[cfg(feature = "sinks-blackhole")]
pub mod blackhole;
#[cfg(feature = "sinks-clickhouse")]
pub mod clickhouse;
#[cfg(feature = "sinks-console")]
pub mod console;
#[cfg(feature = "sinks-datadog")]
pub mod datadog;
#[cfg(feature = "sinks-elasticsearch")]
pub mod elasticsearch;
#[cfg(feature = "sinks-file")]
pub mod file;
#[cfg(feature = "sinks-gcp")]
pub mod gcp;
#[cfg(feature = "sinks-honeycomb")]
pub mod honeycomb;
#[cfg(feature = "sinks-http")]
pub mod http;
#[cfg(feature = "sinks-humio_logs")]
pub mod humio_logs;
#[cfg(feature = "sinks-influxdb")]
pub mod influxdb;
#[cfg(all(feature = "sinks-kafka", feature = "rdkafka"))]
pub mod kafka;
#[cfg(feature = "sinks-logdna")]
pub mod logdna;
#[cfg(feature = "sinks-loki")]
pub mod loki;
#[cfg(feature = "sinks-new_relic_logs")]
pub mod new_relic_logs;
#[cfg(feature = "sinks-papertrail")]
pub mod papertrail;
#[cfg(feature = "sinks-prometheus")]
pub mod prometheus;
#[cfg(feature = "sinks-pulsar")]
pub mod pulsar;
#[cfg(feature = "sinks-sematext")]
pub mod sematext;
#[cfg(feature = "sinks-socket")]
pub mod socket;
#[cfg(feature = "sinks-splunk_hec")]
pub mod splunk_hec;
#[cfg(feature = "sinks-statsd")]
pub mod statsd;
#[cfg(feature = "sinks-vector")]
pub mod vector;

pub enum VectorSink {
    Futures01Sink(Box<dyn futures01::Sink<SinkItem = Event, SinkError = ()> + Send + 'static>),
    Stream(Box<dyn util::StreamSink + Send>),
}

pub type Healthcheck = BoxFuture<'static, crate::Result<()>>;

/// Common build errors
#[derive(Debug, Snafu)]
pub enum BuildError {
    #[snafu(display("Unable to resolve DNS for {:?}", address))]
    DNSFailure { address: String },
    #[snafu(display("DNS errored {}", source))]
    DNSError { source: crate::dns::DnsError },
    #[snafu(display("Socket address problem: {}", source))]
    SocketAddressError { source: std::io::Error },
    #[snafu(display("URI parse error: {}", source))]
    UriParseError { source: ::http::uri::InvalidUri },
}

/// Common healthcheck errors
#[derive(Debug, Snafu)]
pub enum HealthcheckError {
    #[snafu(display("Unexpected status: {}", status))]
    UnexpectedStatus { status: ::http::StatusCode },
}

impl VectorSink {
    pub async fn run<S>(mut self, input: S) -> Result<(), ()>
    where
        S: futures::Stream<Item = Event> + Send + 'static,
    {
        match self {
            Self::Futures01Sink(sink) => {
                // Convert stream03 to stream01 instead of sink01 to sink03 to avoid close issues with Compat01as03Sink
                // See https://github.com/timberio/vector/issues/4256
                let inner = Compat::new(Box::pin(input.map(Ok)));
                inner.forward(sink).compat().map_ok(|_| ()).await
            }
            Self::Stream(ref mut s) => s.run(Box::pin(input)).await,
        }
    }

    pub fn into_futures01sink(
        self,
    ) -> Box<dyn futures01::Sink<SinkItem = Event, SinkError = ()> + Send + 'static> {
        match self {
            Self::Futures01Sink(sink) => sink,
            _ => panic!("Failed type coercion, {:?} is not a Futures01Sink", self),
        }
    }
}

impl fmt::Debug for VectorSink {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("VectorSink").finish()
    }
}
