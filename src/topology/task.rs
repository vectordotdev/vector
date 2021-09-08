use crate::buffers::{Acker, EventStream};
use crate::config::ComponentKey;
use futures::{future::BoxFuture, FutureExt};
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
    Sink(Pin<EventStream>, Acker),
    Healthcheck,
}

/// High level topology task.
#[pin_project]
pub struct Task {
    #[pin]
    inner: BoxFuture<'static, Result<TaskOutput, ()>>,
    id: ComponentKey,
    typetag: String,
}

impl Task {
    pub fn new<S, Fut>(id: ComponentKey, typetag: S, inner: Fut) -> Self
    where
        S: Into<String>,
        Fut: Future<Output = Result<TaskOutput, ()>> + Send + 'static,
    {
        Self {
            inner: inner.boxed(),
            id,
            typetag: typetag.into(),
        }
    }

    pub const fn id(&self) -> &ComponentKey {
        &self.id
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
            .field("id", &self.id.to_string())
            .field("typetag", &self.typetag)
            .finish()
    }
}
