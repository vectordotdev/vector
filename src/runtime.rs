use futures01::future::{ExecuteError, Executor, Future};
use std::io;
use std::pin::Pin;
use tokio::task::JoinHandle;
use tokio_compat::runtime::{Builder, Runtime as TokioRuntime, TaskExecutor as TokioTaskExecutor};

pub struct Runtime {
    rt: TokioRuntime,
}

impl Runtime {
    pub fn new() -> io::Result<Self> {
        Ok(Runtime {
            rt: TokioRuntime::new()?,
        })
    }

    pub fn single_threaded() -> io::Result<Self> {
        Self::with_thread_count(1)
    }

    pub fn with_thread_count(number: usize) -> io::Result<Self> {
        Ok(Runtime {
            rt: Builder::new().core_threads(number).build()?,
        })
    }

    pub fn spawn<F>(&mut self, future: F) -> &mut Self
    where
        F: Future<Item = (), Error = ()> + Send + 'static,
    {
        self.rt.spawn(future);
        self
    }

    pub fn spawn_std<F>(&mut self, future: F) -> &mut Self
    where
        F: std::future::Future<Output = ()> + Send + 'static,
    {
        self.rt.spawn_std(future);
        self
    }

    pub fn spawn_handle_std<F>(&mut self, future: F) -> JoinHandle<F::Output>
    where
        F: std::future::Future + Send + 'static,
        F::Output: Send + 'static,
    {
        self.rt.spawn_handle_std(future)
    }

    pub fn executor(&self) -> TaskExecutor {
        TaskExecutor {
            inner: self.rt.executor(),
        }
    }

    pub fn block_on<F, R, E>(&mut self, future: F) -> Result<R, E>
    where
        F: Send + 'static + Future<Item = R, Error = E>,
        R: Send + 'static,
        E: Send + 'static,
    {
        self.rt.block_on(future)
    }

    pub fn block_on_std<F>(&mut self, future: F) -> F::Output
    where
        F: std::future::Future + Send + 'static,
        F::Output: Send + 'static,
    {
        use futures::future::{FutureExt, TryFutureExt};

        self.rt
            .block_on(future.unit_error().boxed().compat())
            .unwrap()
    }

    pub fn shutdown_on_idle(self) -> impl Future<Item = (), Error = ()> {
        self.rt.shutdown_on_idle()
    }

    pub fn shutdown_now(self) -> impl Future<Item = (), Error = ()> {
        self.rt.shutdown_now()
    }
}

#[derive(Clone, Debug)]
pub struct TaskExecutor {
    inner: TokioTaskExecutor,
}

impl TaskExecutor {
    pub fn spawn(&self, f: impl Future<Item = (), Error = ()> + Send + 'static) {
        self.execute(f).unwrap()
    }

    pub fn spawn_std(&self, f: impl std::future::Future<Output = ()> + Send + 'static) {
        use futures::future::{FutureExt, TryFutureExt};

        self.spawn(f.unit_error().boxed().compat());
    }
}

impl<F> Executor<F> for TaskExecutor
where
    F: Future<Item = (), Error = ()> + Send + 'static,
{
    fn execute(&self, future: F) -> Result<(), ExecuteError<F>> {
        self.inner.execute(future)
    }
}

impl tokio01::executor::Executor for TaskExecutor {
    fn spawn(
        &mut self,
        fut: Box<dyn Future<Item = (), Error = ()> + Send + 'static>,
    ) -> Result<(), tokio01::executor::SpawnError> {
        Ok(self.inner.spawn(fut))
    }
}

pub trait FutureExt: futures::TryFuture {
    /// Used to compat a `!Unpin` type from 0.3 futures to 0.1
    fn boxed_compat(self) -> futures::compat::Compat<Pin<Box<Self>>>
    where
        Self: Sized,
    {
        let fut = Box::pin(self);
        futures::compat::Compat::new(fut)
    }
}

impl<T: futures::TryFuture> FutureExt for T {}
