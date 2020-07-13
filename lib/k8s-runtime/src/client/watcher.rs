//! A [`Watcher`] based on the Kubernetes API [`Client`].

use super::{stream as k8s_stream, watch_request_builder::WatchRequestBuilder, Client};
use crate::watcher;
use futures::{
    future::BoxFuture,
    stream::{BoxStream, Stream},
};
use http::StatusCode;
use http_body::Body;
use k8s_openapi::{WatchOptional, WatchResponse};
use snafu::{ResultExt, Snafu};

/// A simple watcher atop of the Kubernetes API [`Client`].
#[derive(Debug)]
pub struct Watcher<C, B>
where
    C: 'static,
    B: 'static,
{
    client: C,
    request_builder: B,
}

impl<C, B> Watcher<C, B>
where
    C: 'static,
    B: 'static,
{
    /// Create a new [`Watcher`].
    pub fn new(client: C, request_builder: B) -> Self {
        Self {
            client,
            request_builder,
        }
    }
}

impl<C, B> Watcher<C, B>
where
    C: 'static + Client + Send,
    Vec<u8>: Into<<C as Client>::Body>,
    <C as Client>::Error: Send + Unpin,
    <C as Client>::Body: std::fmt::Debug,
    <<C as Client>::Body as Body>::Error: Send + Unpin + std::error::Error,
    B: 'static + WatchRequestBuilder,
    <B as WatchRequestBuilder>::Object: Send + Unpin,
{
    async fn invoke(
        &mut self,
        watch_optional: WatchOptional<'_>,
    ) -> Result<
        impl Stream<
                Item = Result<
                    WatchResponse<<B as WatchRequestBuilder>::Object>,
                    k8s_stream::Error<<<C as Client>::Body as Body>::Error>,
                >,
            > + 'static,
        watcher::invocation::Error<invocation::Error<<C as Client>::Error>>,
    > {
        // Prepare request.
        let request = self
            .request_builder
            .build(watch_optional)
            .context(invocation::RequestPreparation)?;
        trace!(message = "request prepared", ?request);

        // Send request, get response.
        let response = self
            .client
            .send(request)
            .await
            .context(invocation::Request)?;
        trace!(message = "got response", ?response);

        // Handle response status code.
        let status = response.status();
        if status != StatusCode::OK {
            let source = invocation::Error::BadStatus { status };
            let err = if status == StatusCode::GONE {
                watcher::invocation::Error::desync(source)
            } else {
                watcher::invocation::Error::other(source)
            };
            return Err(err);
        }

        // Stream response body.
        let body = response.into_body();
        Ok(k8s_stream::body(body))
    }
}

impl<C, B> watcher::Watcher for Watcher<C, B>
where
    C: 'static + Client + Send,
    Vec<u8>: Into<<C as Client>::Body>,
    <C as Client>::Error: Send + Unpin,
    <C as Client>::Body: Send + Unpin + std::fmt::Debug,
    <<C as Client>::Body as Body>::Error: Send + Unpin + std::error::Error,
    <<C as Client>::Body as Body>::Data: Send + Unpin,
    B: 'static + WatchRequestBuilder + Send,
    <B as WatchRequestBuilder>::Object: Send + Unpin,
{
    type Object = <B as WatchRequestBuilder>::Object;

    type InvocationError = invocation::Error<<C as Client>::Error>;

    type StreamError = k8s_stream::Error<<<C as Client>::Body as Body>::Error>;
    type Stream = BoxStream<'static, Result<WatchResponse<Self::Object>, Self::StreamError>>;

    fn watch<'a>(
        &'a mut self,
        watch_optional: WatchOptional<'a>,
    ) -> BoxFuture<'a, Result<Self::Stream, watcher::invocation::Error<Self::InvocationError>>>
    {
        Box::pin(async move {
            self.invoke(watch_optional)
                .await
                .map(Box::pin)
                .map(|stream| stream as BoxStream<_>)
        })
    }
}

pub mod invocation {
    //! Invocation error.
    use super::*;

    /// Errors that can occur while watching.
    #[derive(Debug, Snafu)]
    #[snafu(visibility(pub))]
    pub enum Error<RequestError>
    where
        RequestError: std::error::Error + 'static,
    {
        /// Returned when the call-specific request builder fails.
        #[snafu(display("failed to prepare an HTTP request"))]
        RequestPreparation {
            /// The underlying error.
            source: k8s_openapi::RequestError,
        },

        /// Returned when the HTTP client fails to perform an HTTP request.
        #[snafu(display("error during the HTTP request"))]
        Request {
            /// The error that API client retunred.
            source: RequestError,
        },

        /// Returned when the HTTP response has a bad status.
        #[snafu(display("HTTP response has a bad status: {}", status))]
        BadStatus {
            /// The status from the HTTP response.
            status: StatusCode,
        },
    }

    impl<RequestError> From<Error<RequestError>> for watcher::invocation::Error<Error<RequestError>>
    where
        RequestError: std::error::Error + 'static + Send,
    {
        fn from(source: Error<RequestError>) -> Self {
            watcher::invocation::Error::other(source)
        }
    }
}
