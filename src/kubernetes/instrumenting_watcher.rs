//! A watcher that adds instrumentation.

use futures::{future::BoxFuture, stream::BoxStream, FutureExt, StreamExt};
use k8s_openapi::{apimachinery::pkg::apis::meta::v1::WatchEvent, WatchOptional};

use super::watcher::{self, Watcher};
use crate::internal_events::kubernetes::instrumenting_watcher as internal_events;

/// A watcher that wraps another watcher with instrumentation calls.
pub struct InstrumentingWatcher<T>
where
    T: Watcher,
{
    inner: T,
}

impl<T> InstrumentingWatcher<T>
where
    T: Watcher,
{
    /// Create a new [`InstrumentingWatcher`].
    pub fn new(inner: T) -> Self {
        Self { inner }
    }
}

impl<T> Watcher for InstrumentingWatcher<T>
where
    T: Watcher,
    <T as Watcher>::Stream: 'static,
{
    type Object = <T as Watcher>::Object;

    type InvocationError = <T as Watcher>::InvocationError;

    type StreamError = <T as Watcher>::StreamError;
    type Stream = BoxStream<
        'static,
        Result<WatchEvent<Self::Object>, watcher::stream::Error<Self::StreamError>>,
    >;

    fn watch<'a>(
        &'a mut self,
        watch_optional: WatchOptional<'a>,
    ) -> BoxFuture<'a, Result<Self::Stream, watcher::invocation::Error<Self::InvocationError>>>
    {
        Box::pin(self.inner.watch(watch_optional).map(|result| {
            result
                .map(|stream| {
                    emit!(&internal_events::WatchRequestInvoked);
                    Box::pin(stream.map(|item_result| {
                        item_result
                            .map(|item| {
                                emit!(&internal_events::WatchStreamItemObtained);
                                item
                            })
                            .map_err(|error| {
                                emit!(&internal_events::WatchStreamFailed { error: &error });
                                error
                            })
                    })) as BoxStream<'static, _>
                })
                .map_err(|error| {
                    emit!(&internal_events::WatchRequestInvocationFailed { error: &error });
                    error
                })
        }))
    }
}
