use futures::Future;

pub mod tcp;

pub type Source = Box<dyn Future<Item = (), Error = ()> + Send>;
