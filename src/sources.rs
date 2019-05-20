use futures::Future;

pub mod file;
pub mod native;
pub mod statsd;
pub mod stdin;
pub mod syslog;
pub mod tcp;
mod util;

pub type Source = Box<dyn Future<Item = (), Error = ()> + Send>;
