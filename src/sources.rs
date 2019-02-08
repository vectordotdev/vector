use futures::Future;

pub mod file;
pub mod tcp;

pub type Source = Box<dyn Future<Item = (), Error = ()> + Send>;
