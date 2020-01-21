use futures::Future;
use snafu::Snafu;

pub mod docker;
pub mod file;
pub mod journald;
#[cfg(feature = "rdkafka")]
pub mod kafka;
pub mod kubernetes;
pub mod logplex;
pub mod prometheus;
pub mod socket;
pub mod splunk_hec;
pub mod statsd;
pub mod stdin;
pub mod syslog;
mod util;
pub mod vector;

pub type Source = Box<dyn Future<Item = (), Error = ()> + Send>;

/// Common build errors
#[derive(Debug, Snafu)]
enum BuildError {
    #[snafu(display("URI parse error: {}", source))]
    UriParseError { source: ::http::uri::InvalidUri },
}
