use std::future::Future;

use futures::future::BoxFuture;

/// Error returned by a running source.
pub type SourceError = vector_common::Error;

/// Future that runs a source until shutdown or failure.
pub type Source = BoxFuture<'static, Result<(), SourceError>>;

/// Adapt a source that only reports whether it failed.
pub fn opaque_source<F>(future: F) -> Source
where
    F: Future<Output = Result<(), ()>> + Send + 'static,
{
    Box::pin(async move {
        future
            .await
            .map_err(|()| "the task completed with an error".into())
    })
}
