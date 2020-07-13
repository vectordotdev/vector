//! Watcher abstraction.

use futures::{future::BoxFuture, stream::Stream};
use k8s_openapi::{Resource, WatchOptional, WatchResponse};
use serde::de::DeserializeOwned;
use snafu::Snafu;

#[cfg(any(test, feature = "mock-watcher"))]
pub mod mock;
#[cfg(any(test, feature = "mock-watcher"))]
pub use mock::Mock;

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
    ) -> BoxFuture<'a, Result<Self::Stream, invocation::Error<Self::InvocationError>>>;
}

pub mod invocation {
    //! Invocation errors.
    use super::*;

    /// Error wrapper providing a semantic wrapper around invocation errors to
    /// bind meaningful and actionably common error semantics to the arbitrary
    /// underlying errors.
    #[derive(Debug, Snafu)]
    #[snafu(visibility(pub))]
    pub enum Error<T>
    where
        T: std::error::Error + Send + 'static,
    {
        /// Desync error signals that the server went out of sync and the resource
        /// version specified in the call can no longer be used.
        Desync {
            /// The underlying error.
            source: T,
        },

        /// Any other error that may have maening for downstream but doesn't have
        /// a semantics attached to it at the [`Watcher`] trait level.
        Other {
            /// The underlying error.
            source: T,
        },
    }

    impl<T> Error<T>
    where
        T: std::error::Error + Send + 'static,
    {
        /// Create an `Error::Desync`.
        #[inline]
        pub fn desync(source: T) -> Self {
            Self::Desync { source }
        }

        /// Create an `Error::Other`.
        #[inline]
        pub fn other(source: T) -> Self {
            Self::Other { source }
        }
    }
}
