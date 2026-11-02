use std::{
    any::Any,
    fmt,
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};

use futures::{FutureExt, future::BoxFuture};
use pin_project::pin_project;
use snafu::Snafu;
use tokio::task::JoinError;
use vector_lib::{
    buffers::topology::channel::BufferReceiverStream, event::EventArray, sink::SinkError,
};

use crate::{config::ComponentKey, utilization::Utilization};

#[allow(clippy::large_enum_variant)]
pub(crate) enum TaskOutput {
    Source,
    Transform,
    /// Buffer of sink
    Sink(Utilization<BufferReceiverStream<EventArray>>),
    Healthcheck,
}

#[derive(Debug, Snafu)]
pub(crate) enum TaskError {
    #[snafu(display("the task was cancelled before it completed"))]
    Cancelled,
    #[snafu(display("the task panicked and was aborted"))]
    Panicked { panic: String },
    #[snafu(display("{}", source))]
    Wrapped { source: crate::Error },
}

impl TaskError {
    pub fn wrapped(e: crate::Error) -> Self {
        Self::Wrapped { source: e }
    }

    pub fn panicked(payload: &(dyn Any + Send)) -> Self {
        let panic = payload
            .downcast_ref::<String>()
            .map(String::as_str)
            .or_else(|| payload.downcast_ref::<&str>().copied())
            .unwrap_or("non-string panic payload")
            .to_owned();
        Self::Panicked { panic }
    }
}

impl From<crate::Error> for TaskError {
    fn from(error: crate::Error) -> Self {
        Self::wrapped(error)
    }
}

impl From<SinkError> for TaskError {
    fn from(error: SinkError) -> Self {
        Self::wrapped(error.into_inner())
    }
}

impl From<JoinError> for TaskError {
    fn from(e: JoinError) -> Self {
        if e.is_cancelled() {
            Self::Cancelled
        } else {
            Self::panicked(e.into_panic().as_ref())
        }
    }
}

pub(crate) type TaskResult = Result<TaskOutput, TaskError>;

/// High level topology task.
#[pin_project]
pub(crate) struct Task {
    #[pin]
    inner: BoxFuture<'static, TaskResult>,
    key: ComponentKey,
    typetag: String,
}

impl Task {
    pub fn new<S, Fut>(key: ComponentKey, typetag: S, inner: Fut) -> Self
    where
        S: Into<String>,
        Fut: Future<Output = TaskResult> + Send + 'static,
    {
        Self {
            inner: inner.boxed(),
            key,
            typetag: typetag.into(),
        }
    }

    pub fn id(&self) -> &str {
        self.key.id()
    }

    pub const fn typetag(&self) -> &str {
        self.typetag.as_str()
    }
}

impl Future for Task {
    type Output = TaskResult;

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
            .finish_non_exhaustive()
    }
}
