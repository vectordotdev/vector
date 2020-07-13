//! A client interface suitable for performing HTTP requests to the
//! Kubernetes API server.

use async_trait::async_trait;
use http::{Request, Response};
use http_body::Body;
use std::error::Error;

pub mod watch_request_builder;
pub mod watcher;

mod multi_response_decoder;
mod stream;

pub use watcher::Watcher;

/// A client interface suitable for performing HTTP requests to the
/// Kubernetes API server.
#[async_trait]
pub trait Client {
    /// The body type used in requests and responses.
    type Body: Body;
    /// The error type that can occur during request processing.
    type Error: Error;

    /// Send and HTTP [`Request`] `req` and return a [`Response`] or an error.
    async fn send<B>(&mut self, req: Request<B>) -> Result<Response<Self::Body>, Self::Error>
    where
        B: Into<Self::Body> + Send;
}
