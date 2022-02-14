use std::{
    fmt,
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};

use futures::{future::BoxFuture, FutureExt};
use pin_project::pin_project;
use vector_core::{
    buffers::{topology::channel::BufferReceiver, Acker},
    event::Event,
};

use crate::{config::ComponentKey, utilization::Utilization};

pub enum TaskOutput {
    Source,
    Transform,
    /// Buffer of sink
    Sink(Utilization<BufferReceiver<Event>>, Acker),
    Healthcheck,
}

/// High level topology task.
#[pin_project]
pub struct Task {
    #[pin]
    inner: BoxFuture<'static, Result<TaskOutput, ()>>,
    key: ComponentKey,
    typetag: String,
}

impl Task {
    pub fn new<S, Fut>(key: ComponentKey, typetag: S, inner: Fut) -> Self
    where
        S: Into<String>,
        Fut: Future<Output = Result<TaskOutput, ()>> + Send + 'static,
    {
        Self {
            inner: inner.boxed(),
            key,
            typetag: typetag.into(),
        }
    }

    pub const fn key(&self) -> &ComponentKey {
        &self.key
    }

    pub fn id(&self) -> &str {
        self.key.id()
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
            .field("id", &self.key.id().to_string())
            .field("typetag", &self.typetag)
            .finish()
    }
}
