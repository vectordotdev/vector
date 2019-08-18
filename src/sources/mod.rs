use futures::Future;

pub mod file;
#[cfg(feature = "rdkafka")]
pub mod kafka;
pub mod statsd;
pub mod stdin;
pub mod syslog;
pub mod tcp;
pub mod udp;
mod util;
pub mod vector;

pub type Source = Box<dyn Future<Item = (), Error = ()> + Send>;
