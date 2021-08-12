use crate::buffers::{Acker, EventStream};
use crate::config::ComponentScope;
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
    scope: ComponentScope,
    typetag: String,
}

impl Task {
    pub fn new<S, Fut>(scope: ComponentScope, typetag: S, inner: Fut) -> Self
    where
        S: Into<String>,
        Fut: Future<Output = Result<TaskOutput, ()>> + Send + 'static,
    {
        Self {
            inner: inner.boxed(),
            scope,
            typetag: typetag.into(),
        }
    }

    pub fn name(&self) -> &str {
        &self.scope.name()
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
            .field("name", &self.scope.name())
            .field("scope", self.scope.scope())
            .field("typetag", &self.typetag)
            .finish()
    }
}
