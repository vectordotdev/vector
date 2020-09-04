use crate::Event;
use futures::{
    compat::{Compat, Future01CompatExt},
    StreamExt, TryFutureExt,
};
use snafu::Snafu;
use std::fmt;

pub mod streaming_sink;
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
#[cfg(feature = "sinks-sematext_logs")]
pub mod sematext_logs;
#[cfg(feature = "sinks-socket")]
pub mod socket;
#[cfg(feature = "sinks-splunk_hec")]
pub mod splunk_hec;
#[cfg(feature = "sinks-statsd")]
pub mod statsd;
#[cfg(feature = "sinks-vector")]
pub mod vector;

pub enum VectorSink {
    Futures01Sink(Box<dyn futures01::Sink<SinkItem = Event, SinkError = ()> + 'static + Send>),
}

pub type Healthcheck = Box<dyn futures01::Future<Item = (), Error = crate::Error> + Send>;

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
    pub async fn run01<S>(self, input: S) -> Result<(), ()>
    where
        S: futures01::Stream<Item = Event, Error = ()>,
    {
        match self {
            Self::Futures01Sink(sink) => input.forward(sink).compat().map_ok(|_| ()).await,
        }
    }

    pub async fn run<S>(self, input: S) -> Result<(), ()>
    where
        S: futures::Stream<Item = Result<Event, ()>> + Send,
    {
        self.run01(Compat::new(input.boxed())).await
    }

    pub fn into_futures01sink(
        self,
    ) -> Box<dyn futures01::Sink<SinkItem = Event, SinkError = ()> + 'static + Send> {
        match self {
            Self::Futures01Sink(sink) => sink,
            // _ => panic!("Failed type coercion, {:?} is not a Futures01Sink", self),
        }
    }
}

impl fmt::Debug for VectorSink {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("VectorSink").finish()
    }
}
