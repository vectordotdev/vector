use std::future::Future;

use futures::{TryFutureExt, future::BoxFuture};

/// Error returned by a running source.
pub type SourceError = vector_common::Error;

/// Future that runs a source until shutdown or failure.
pub type Source = BoxFuture<'static, Result<(), SourceError>>;

fn default_source_error() -> SourceError {
    "the source task completed with an error".into()
}

/// Adapt a source that only reports whether it failed.
pub fn opaque_source<F>(future: F) -> Source
where
    F: Future<Output = Result<(), ()>> + Send + 'static,
{
    Box::pin(future.map_err(|()| default_source_error()))
}

/// Adapt a source that returns an error with a cause.
pub fn source_with_error<F, E>(future: F) -> Source
where
    F: Future<Output = Result<(), E>> + Send + 'static,
    E: Into<SourceError>,
{
    Box::pin(future.err_into())
}
