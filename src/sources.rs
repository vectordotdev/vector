use futures::Future;

pub mod file;
pub mod stdin;
pub mod syslog;
pub mod tcp;
mod util;

pub type Source = Box<dyn Future<Item = (), Error = ()> + Send>;
