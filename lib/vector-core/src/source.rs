use futures::future::BoxFuture;

pub type Source = BoxFuture<'static, Result<(), ()>>;
