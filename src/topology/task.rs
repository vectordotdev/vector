use crate::{buffers::Acker, event::Event};
use futures::{future::BoxFuture, FutureExt, Stream};
use pin_project::pin_project;
use std::{
    fmt,
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};

pub enum TaskOutput {
    Source,
    Transform,
    /// Buffer of sink
    Sink(Pin<Box<dyn Stream<Item = Event> + Send>>, Acker),
    Healthcheck,
}

/// High level topology task.
#[pin_project]
pub struct Task {
    #[pin]
    inner: BoxFuture<'static, Result<TaskOutput, ()>>,
    name: String,
    typetag: String,
}

impl Task {
    pub fn new<S1, S2, Fut>(name: S1, typetag: S2, inner: Fut) -> Self
    where
        S1: Into<String>,
        S2: Into<String>,
        Fut: Future<Output = Result<TaskOutput, ()>> + Send + 'static,
    {
        Self {
            inner: inner.boxed(),
            name: name.into(),
            typetag: typetag.into(),
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn typetag(&self) -> &str {
        &self.typetag
    }
}

impl Future for Task {
    type Output = Result<TaskOutput, ()>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this: &mut Task = self.get_mut();
        this.inner.as_mut().poll(cx)
    }
}

impl fmt::Debug for Task {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Task")
            .field("name", &self.name)
            .field("typetag", &self.typetag)
            .finish()
    }
}
