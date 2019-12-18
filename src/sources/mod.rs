use futures::Future;

pub mod docker;
pub mod file;
pub mod journald;
#[cfg(feature = "rdkafka")]
pub mod kafka;
pub mod socket;
pub mod splunk_hec;
pub mod statsd;
pub mod stdin;
pub mod syslog;
pub mod tcp;
mod util;
pub mod vector;

pub type Source = Box<dyn Future<Item = (), Error = ()> + Send>;
