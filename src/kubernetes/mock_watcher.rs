//! A mock watcher.

#![cfg(test)]

use super::watcher::{self, Watcher};
use futures::{
    future::{self, BoxFuture},
    stream::BoxStream,
    Stream,
};
use k8s_openapi::{WatchOptional, WatchResponse};
use pin_project::pin_project;
use serde::de::DeserializeOwned;
use std::marker::PhantomData;
use std::{
    fmt,
    pin::Pin,
    task::{Context, Poll},
};

/// A mock watcher, useful for tests.
#[derive(Debug)]
pub struct MockWatcher<T, FI> {
    data_type: PhantomData<T>,
    invoke_fn: FI,
}

impl<T, FI> MockWatcher<T, FI> {
    /// Create a new [`MockWatcher`].
    pub fn new(invoke_fn: FI) -> Self {
        let data_type = PhantomData;
        Self {
            data_type,
            invoke_fn,
        }
    }
}

impl<T, FI, FS> Watcher for MockWatcher<T, FI>
where
    T: DeserializeOwned + Send + Sync + 'static,
    FI: for<'a> FnMut(WatchOptional<'a>) -> Result<FS, watcher::invocation::Error<InvocationError>>
        + Send
        + 'static,
    FS: FnMut() -> Option<Result<WatchResponse<T>, StreamError>> + Send + Sync + Unpin + 'static,
{
    type Object = T;

    type StreamError = StreamError;
    type Stream = BoxStream<'static, Result<WatchResponse<Self::Object>, Self::StreamError>>;

    type InvocationError = InvocationError;

    fn watch<'a>(
        &'a mut self,
        watch_optional: WatchOptional<'a>,
    ) -> BoxFuture<'a, Result<Self::Stream, watcher::invocation::Error<Self::InvocationError>>>
    {
        let result = (self.invoke_fn)(watch_optional);
        let result = result.map(|stream_fn| {
            Box::pin(MockWatcherStream {
                stream_fn,
                data_type: PhantomData,
            }) as Self::Stream
        });
        Box::pin(future::ready(result))
    }
}

#[pin_project]
struct MockWatcherStream<T, FS> {
    data_type: PhantomData<T>,
    stream_fn: FS,
}

impl<T, FS> Stream for MockWatcherStream<T, FS>
where
    T: DeserializeOwned + Send + Sync,
    FS: FnMut() -> Option<Result<WatchResponse<T>, StreamError>> + Send + Sync + Unpin + 'static,
{
    type Item = Result<WatchResponse<T>, StreamError>;

    fn poll_next(self: Pin<&mut Self>, _cx: &mut Context) -> Poll<Option<Self::Item>> {
        let this = self.project();
        Poll::Ready((this.stream_fn)())
    }
}

/// An error kind for the mock watcher invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvocationError;

/// An error kind for the mock watcher stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreamError;

impl fmt::Display for InvocationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}

impl fmt::Display for StreamError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}

impl std::error::Error for InvocationError {}
impl std::error::Error for StreamError {}
