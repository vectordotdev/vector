use futures::Future;

pub mod file;
pub mod syslog;
pub mod tcp;

pub type Source = Box<dyn Future<Item = (), Error = ()> + Send>;
