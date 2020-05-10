use futures01::Future;
use snafu::Snafu;

#[cfg(feature = "sources-docker")]
pub mod docker;
#[cfg(feature = "sources-file")]
pub mod file;
#[cfg(feature = "sources-generator")]
pub mod generator;
#[cfg(feature = "sources-http")]
pub mod http;
#[cfg(feature = "sources-internal_metrics")]
pub mod internal_metrics;
#[cfg(all(feature = "sources-journald", feature = "unix"))]
pub mod journald;
#[cfg(all(feature = "sources-kafka", feature = "rdkafka"))]
pub mod kafka;
#[cfg(feature = "sources-logplex")]
pub mod logplex;
#[cfg(feature = "sources-prometheus")]
pub mod prometheus;
#[cfg(feature = "sources-socket")]
pub mod socket;
#[cfg(feature = "sources-splunk_hec")]
pub mod splunk_hec;
#[cfg(feature = "sources-statsd")]
pub mod statsd;
#[cfg(feature = "sources-stdin")]
pub mod stdin;
#[cfg(feature = "sources-syslog")]
pub mod syslog;
#[cfg(feature = "sources-vector")]
pub mod vector;

mod util;

pub type Source = Box<dyn Future<Item = (), Error = ()> + Send>;

/// Common build errors
#[derive(Debug, Snafu)]
enum BuildError {
    #[snafu(display("URI parse error: {}", source))]
    UriParseError { source: ::http::uri::InvalidUri },
}
