//! A mock watcher.

#![cfg(test)]

use std::fmt;

use async_stream::try_stream;
use futures::{
    channel::mpsc::{Receiver, Sender},
    future::BoxFuture,
    stream::BoxStream,
    SinkExt, StreamExt,
};
use k8s_openapi::{apimachinery::pkg::apis::meta::v1::WatchEvent, Resource, WatchOptional};
use serde::de::DeserializeOwned;

use super::watcher::{self, Watcher};

/// An event that's send to the test scenario driver.
#[derive(Debug, PartialEq)]
pub enum ScenarioEvent {
    Invocation(OwnedWatchOptional),
    Stream,
}

/// An action that's send from the test scenario driver to specify the
/// invocation result.
pub enum ScenarioActionInvocation<T>
where
    T: DeserializeOwned + Resource,
{
    /// Return successfully and prepare the stream with responses from the
    /// passed [`Receiver`].
    Ok(Receiver<ScenarioActionStream<T>>),
    /// Return a desync error.
    ErrDesync,
    /// Return a recoverable error.
    ErrRecoverable,
    /// Return an "other" (i.e. non-desync) error.
    ErrOther,
}

/// An action that's send from the test scenario driver to specify the
/// stream item request result.
pub enum ScenarioActionStream<T>
where
    T: DeserializeOwned + Resource,
{
    /// Return a watch response.
    Ok(WatchEvent<T>),
    /// Return a desync error.
    ErrDesync,
    /// Return a recoverable error.
    ErrRecoverable,
    /// Complete the stream (return `None`).
    Done,
}

/// A mock watcher, useful for tests.
pub struct MockWatcher<T>
where
    T: DeserializeOwned + Resource,
{
    events_tx: Sender<ScenarioEvent>,
    invocation_rx: Receiver<ScenarioActionInvocation<T>>,
}

impl<T> MockWatcher<T>
where
    T: DeserializeOwned + Resource,
{
    /// Create a new [`MockWatcher`].
    pub fn new(
        events_tx: Sender<ScenarioEvent>,
        invocation_rx: Receiver<ScenarioActionInvocation<T>>,
    ) -> Self {
        Self {
            events_tx,
            invocation_rx,
        }
    }
}

impl<T> Watcher for MockWatcher<T>
where
    T: DeserializeOwned + Resource + Send + Sync + Unpin + 'static,
{
    type Object = T;

    type StreamError = StreamError;
    type Stream = BoxStream<
        'static,
        Result<WatchEvent<Self::Object>, watcher::stream::Error<Self::StreamError>>,
    >;

    type InvocationError = InvocationError;

    fn watch<'a>(
        &'a mut self,
        watch_optional: WatchOptional<'a>,
    ) -> BoxFuture<'a, Result<Self::Stream, watcher::invocation::Error<Self::InvocationError>>>
    {
        let mut stream_events_tx = self.events_tx.clone();
        Box::pin(async move {
            self.events_tx
                .send(ScenarioEvent::Invocation(watch_optional.into()))
                .await
                .unwrap();
            let action = self.invocation_rx.next().await.unwrap();
            match action {
                ScenarioActionInvocation::Ok(mut stream_rx) => {
                    let stream = Box::pin(try_stream! {
                        loop {
                            stream_events_tx.send(ScenarioEvent::Stream)
                                .await
                                .unwrap();

                            let action = stream_rx.next().await.unwrap();
                            match action {
                                ScenarioActionStream::Ok(val) => {
                                    yield val
                                },
                                ScenarioActionStream::ErrDesync => {
                                    Err(watcher::stream::Error::desync(StreamError))?;
                                    break;
                                },
                                ScenarioActionStream::ErrRecoverable => {
                                    Err(watcher::stream::Error::recoverable(StreamError))?;
                                    break;
                                }
                                ScenarioActionStream::Done => break,
                            }
                        }
                    })
                        as BoxStream<
                            'static,
                            Result<WatchEvent<Self::Object>, watcher::stream::Error<StreamError>>,
                        >;
                    Ok(stream)
                }
                ScenarioActionInvocation::ErrDesync => {
                    Err(watcher::invocation::Error::desync(InvocationError))
                }
                ScenarioActionInvocation::ErrRecoverable => {
                    Err(watcher::invocation::Error::recoverable(InvocationError))
                }
                ScenarioActionInvocation::ErrOther => {
                    Err(watcher::invocation::Error::other(InvocationError))
                }
            }
        })
    }
}

/// An owned variant of [`WatchOptional`].
/// Used to send it with [`ScenarioEvent`] to avoid the headaches with
/// lifetimes.
#[derive(Debug, PartialEq, Eq)]
pub struct OwnedWatchOptional {
    pub allow_watch_bookmarks: Option<bool>,
    pub field_selector: Option<String>,
    pub label_selector: Option<String>,
    pub pretty: Option<String>,
    pub resource_version: Option<String>,
    pub timeout_seconds: Option<i64>,
}

impl<'a> From<WatchOptional<'a>> for OwnedWatchOptional {
    fn from(val: WatchOptional<'a>) -> Self {
        Self {
            allow_watch_bookmarks: val.allow_watch_bookmarks,
            field_selector: val.field_selector.map(ToOwned::to_owned),
            label_selector: val.label_selector.map(ToOwned::to_owned),
            pretty: val.pretty.map(ToOwned::to_owned),
            resource_version: val.resource_version.map(ToOwned::to_owned),
            timeout_seconds: val.timeout_seconds,
        }
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
