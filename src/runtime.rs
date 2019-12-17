use futures::future::{ExecuteError, Executor, Future};
use std::io;
use tokio::runtime::Builder;

pub struct Runtime {
    rt: tokio::runtime::Runtime,
}

impl Runtime {
    pub fn new() -> io::Result<Self> {
        Ok(Runtime {
            rt: tokio::runtime::Runtime::new()?,
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
        use futures03::future::{FutureExt, TryFutureExt};

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
    inner: tokio::runtime::TaskExecutor,
}

impl TaskExecutor {
    pub fn spawn(&self, f: impl Future<Item = (), Error = ()> + Send + 'static) {
        self.execute(f).unwrap()
    }

    pub fn spawn_std(&self, f: impl std::future::Future<Output = ()> + Send + 'static) {
        use futures03::future::{FutureExt, TryFutureExt};

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
