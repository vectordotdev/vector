//! Watcher abstraction.

use futures::{future::BoxFuture, stream::Stream};
use k8s_openapi::{apimachinery::pkg::apis::meta::v1::WatchEvent, Resource, WatchOptional};
use serde::de::DeserializeOwned;
use snafu::Snafu;

/// Watch over the changes for a k8s resource.
pub trait Watcher {
    /// The type of the watched object.
    type Object: DeserializeOwned + Resource;

    /// The error type watcher invocation implementation uses internally.
    type InvocationError: std::error::Error + Send + 'static;

    /// The error type watcher stream implementation uses internally.
    type StreamError: std::error::Error + Send + 'static;

    /// The stream type produced by the watch request.
    type Stream: Stream<Item = Result<WatchEvent<Self::Object>, stream::Error<Self::StreamError>>>
        + Send;

    /// Issues a single watch request and returns a stream results.
    fn watch<'a>(
        &'a mut self,
        watch_optional: WatchOptional<'a>,
    ) -> BoxFuture<'a, Result<Self::Stream, invocation::Error<Self::InvocationError>>>;
}

pub mod stream {
    //! Stream errors.
    use super::*;

    /// Error wrapper providing a semantic wrapper around stream errors to
    /// bind meaningful and actionable common error semantics to the arbitrary
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
        /// Errors that signal a possibility they can be recovered and as such should be
        /// logged and bubbled up but shouldn't stop processing.
        Recoverable {
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

        /// Create an `Error::Recoverable`.
        #[inline]
        pub fn recoverable(source: T) -> Self {
            Self::Recoverable { source }
        }
    }

    impl<T> PartialEq for Error<T>
    where
        T: std::error::Error + Send + 'static + PartialEq,
    {
        fn eq(&self, other: &Self) -> bool {
            match (self, other) {
                (Error::Desync { source: a }, Error::Desync { source: b })
                | (Error::Recoverable { source: a }, Error::Recoverable { source: b }) => a.eq(b),
                _ => false,
            }
        }
    }

    impl<T> Eq for Error<T> where T: std::error::Error + Send + 'static + Eq {}
}

pub mod invocation {
    //! Invocation errors.
    use super::*;

    /// Error wrapper providing a semantic wrapper around invocation errors to
    /// bind meaningful and actionable common error semantics to the arbitrary
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
        /// Errors that signal a possibility they can be recovered and as such should be
        /// logged and bubbled up but shouldn't stop processing.
        Recoverable {
            /// The underlying error.
            source: T,
        },
        /// Any other error that may have meaning for downstream but doesn't have
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

        /// Create an `Error::Recoverable`.
        #[inline]
        pub fn recoverable(source: T) -> Self {
            Self::Recoverable { source }
        }

        /// Create an `Error::Other`.
        #[inline]
        pub fn other(source: T) -> Self {
            Self::Other { source }
        }
    }

    impl<T> PartialEq for Error<T>
    where
        T: std::error::Error + Send + 'static + PartialEq,
    {
        fn eq(&self, other: &Self) -> bool {
            match (self, other) {
                (Error::Desync { source: a }, Error::Desync { source: b })
                | (Error::Recoverable { source: a }, Error::Recoverable { source: b })
                | (Error::Other { source: a }, Error::Other { source: b }) => a.eq(b),
                _ => false,
            }
        }
    }

    impl<T> Eq for Error<T> where T: std::error::Error + Send + 'static + Eq {}
}
