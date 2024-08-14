use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
    task::{Context, Poll},
};

use http::{Request, Response};
use hyper::Body;
use std::future::Future;
use std::pin::Pin;

use tonic::{
    Status,
    body::BoxBody,
    transport::server::NamedService,
};
use tower::{Layer, Service};

/// A service that tracks the number of requests and elapsed time,
/// shutting down the connection gracefully if the configured limits are reached.
#[derive(Clone)]
pub struct ConnectionLimit<S> {
    inner: S,
    request_count: Arc<Mutex<usize>>,
    max_requests: usize,
    max_duration: Duration,
    start_time: Instant,
}

impl<S> ConnectionLimit<S> {
    pub fn new(inner: S, max_requests: usize, max_duration: Duration) -> Self {
        Self {
            inner,
            request_count: Arc::new(Mutex::new(0)),
            max_requests: max_requests,
            max_duration: max_duration,
            start_time: Instant::now(),
        }
    }
}

impl<S> Service<Request<Body>> for ConnectionLimit<S>
where
    S: Service<Request<Body>, Response = Response<BoxBody>, Error = tonic::Status>
        + NamedService
        + Clone
        + Send
        + 'static,
    S::Future: Send + 'static,
{
    type Response = Response<BoxBody>;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request<Body>) -> Self::Future {
        let max_requests = self.max_requests;
        let max_duration = self.max_duration;
        let request_count = Arc::clone(&self.request_count);
        let start_time = self.start_time;

        let elapsed_time = start_time.elapsed();

        let future = self.inner.call(req);

        Box::pin(async move {
            let response = future.await?;

            // After processing the request, increment the request count and check the limits.
            let mut count = request_count.lock().unwrap();
            *count += 1;

            if *count > max_requests || elapsed_time > max_duration {
                // If the limit is reached, return a ResourceExhausted error to close the connection.
                return Err(Status::resource_exhausted(
                    "Connection closed after reaching the limit.",
                ));
            }

            Ok(response)
        })
    }
}

/// A layer that adds the ConnectionLimit functionality to a service.
#[derive(Clone, Default)]
pub struct ConnectionLimitLayer {
    max_requests: usize,
    max_duration: Duration,
}

impl ConnectionLimitLayer {
    pub fn new(max_requests: usize, max_duration: Duration) -> Self {
        Self {
            max_requests,
            max_duration,
        }
    }
}

impl<S> Layer<S> for ConnectionLimitLayer {
    type Service = ConnectionLimit<S>;

    fn layer(&self, inner: S) -> Self::Service {
        ConnectionLimit::new(inner, self.max_requests, self.max_duration)
    }
}
