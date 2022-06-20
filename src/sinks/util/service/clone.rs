use std::{
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
    task::{Context, Poll},
};

use futures::ready;
use pin_project::pin_project;
use tower::{Layer, Service};

/// Makes otherwise uncloneable `Service` cloneable.
/// This is done through blocking Mutex.
pub struct CloneLayer;

impl<S> Layer<S> for CloneLayer {
    type Service = CloneService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        CloneService::new(inner)
    }
}

/// Service of CloneLayer.
pub struct CloneService<S> {
    inner: Arc<Mutex<S>>,
}

impl<S> CloneService<S> {
    pub fn new(inner: S) -> Self {
        CloneService {
            inner: Arc::new(Mutex::new(inner)),
        }
    }
}

impl<S, Req> Service<Req> for CloneService<S>
where
    S: Service<Req>,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = PollReadyFuture<S, Req>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner
            .lock()
            .expect("Service mutex is poisoned")
            .poll_ready(cx)
    }

    fn call(&mut self, req: Req) -> Self::Future {
        PollReadyFuture::PollReady(Arc::clone(&self.inner), Some(req))
    }
}

impl<S> Clone for CloneService<S> {
    fn clone(&self) -> Self {
        CloneService {
            inner: Arc::clone(&self.inner),
        }
    }
}

#[pin_project(project = PollReadyProj, project_replace = PollReadyOwn)]
pub enum PollReadyFuture<S: Service<Req>, Req> {
    PollReady(Arc<Mutex<S>>, Option<Req>),
    Called(#[pin] S::Future),
}

impl<S, Req> Future for PollReadyFuture<S, Req>
where
    S: Service<Req>,
{
    type Output = Result<S::Response, S::Error>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.as_mut().project();
        match this {
            PollReadyProj::PollReady(inner, req) => {
                let mut inner = inner.lock().expect("Service mutex is poisoned");
                // Poll_ready must be called again since some other thread
                // could have used it up with its own call.
                ready!(inner.poll_ready(cx)?);

                let fut = inner.call(req.take().expect("Request is taken"));
                drop(inner);

                self.as_mut().project_replace(PollReadyFuture::Called(fut));
                self.poll(cx)
            }
            PollReadyProj::Called(future) => future.poll(cx),
        }
    }
}
