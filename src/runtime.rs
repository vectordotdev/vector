use futures01::future::{ExecuteError, Executor, Future};
use std::io;
use std::pin::Pin;
use tokio::{runtime::Handle, task::JoinHandle};
use tokio_compat::runtime::{Builder, Runtime as TokioRuntime, TaskExecutor as TokioTaskExecutor};

pub struct Runtime {
    rt: TokioRuntime,
    handle: Handle,
}

impl Runtime {
    pub fn new() -> io::Result<Self> {
        let mut rt = TokioRuntime::new()?;

        let handle = rt.block_on_std(async move { Handle::current() });

        Ok(Runtime { rt, handle })
    }

    pub fn single_threaded() -> io::Result<Self> {
        Self::with_thread_count(2)
    }

    pub fn with_thread_count(number: usize) -> io::Result<Self> {
        let mut rt = Builder::new().core_threads(number).build()?;

        let handle = rt.block_on_std(async move { Handle::current() });

        Ok(Runtime { rt, handle })
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

    pub fn spawn_handle<F>(&mut self, future: F) -> JoinHandle<F::Output>
    where
        F: std::future::Future + Send + 'static,
        F::Output: Send + 'static,
    {
        self.rt.spawn_handle_std(future)
    }

    pub fn executor(&self) -> TaskExecutor {
        TaskExecutor {
            inner: self.rt.executor(),
            handle: self.handle.clone(),
        }
    }

    pub fn block_on<F>(&mut self, future: F) -> Result<F::Item, F::Error>
    where
        F: Future,
    {
        self.rt.block_on(future)
    }

    pub fn block_on_std<F>(&mut self, future: F) -> F::Output
    where
        F: std::future::Future,
    {
        self.rt.block_on_std(future)
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
    handle: Handle,
}

impl TaskExecutor {
    pub fn spawn(&self, f: impl Future<Item = (), Error = ()> + Send + 'static) {
        self.execute(f).unwrap()
    }

    pub fn spawn_std(&self, f: impl std::future::Future<Output = ()> + Send + 'static) {
        use futures::future::{FutureExt, TryFutureExt};

        self.spawn(f.unit_error().boxed().compat());
    }

    pub fn block_on_std<F: std::future::Future>(&self, f: F) -> F::Output {
        self.handle.block_on(f)
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
