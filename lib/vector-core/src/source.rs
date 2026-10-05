use std::future::Future;

use futures::future::BoxFuture;

/// Error returned by a running source.
#[derive(Debug)]
pub enum SourceError {
    /// The source did not retain its failure reason.
    Opaque,
    /// The error that stopped the source.
    Detailed(vector_common::Error),
}

impl<E> From<E> for SourceError
where
    E: std::error::Error + Send + Sync + 'static,
{
    fn from(error: E) -> Self {
        Self::Detailed(Box::new(error))
    }
}

/// Future that runs a source until shutdown or failure.
pub type Source = BoxFuture<'static, Result<(), SourceError>>;

/// Adapt a source that only reports whether it failed.
pub fn opaque_source<F>(future: F) -> Source
where
    F: Future<Output = Result<(), ()>> + Send + 'static,
{
    Box::pin(async move { future.await.map_err(|()| SourceError::Opaque) })
}
