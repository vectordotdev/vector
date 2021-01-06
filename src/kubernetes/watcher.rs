//! Watcher abstraction.

use futures::{future::BoxFuture, stream::Stream};
use k8s_openapi::{Resource, WatchOptional, WatchResponse};
use serde::de::DeserializeOwned;

/// Watch over the changes for a k8s resource.
pub trait Watcher {
    /// The type of the watched object.
    type Object: DeserializeOwned + Resource;

    /// The error type watcher invocation implementation uses internally.
    type InvocationError: std::error::Error + Send + 'static;

    /// The error type watcher stream implementation uses internally.
    type StreamError: std::error::Error + Send + 'static;

    /// The stream type produced by the watch request.
    type Stream: Stream<Item = Result<WatchResponse<Self::Object>, Self::StreamError>> + Send;

    /// Issues a single watch request and returns a stream results.
    fn watch<'a>(
        &'a mut self,
        watch_optional: WatchOptional<'a>,
    ) -> BoxFuture<'a, Result<Self::Stream, Self::InvocationError>>;
}
