use futures01::future::Future;
use std::io;
use tokio::task::JoinHandle;
use tokio_compat::runtime::{Builder, Runtime as TokioRuntime};

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

    pub fn spawn_handle_std<F>(&mut self, future: F) -> JoinHandle<F::Output>
    where
        F: std::future::Future + Send + 'static,
        F::Output: Send + 'static,
    {
        self.rt.spawn_handle_std(future)
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

    pub fn shutdown_now(self) -> impl Future<Item = (), Error = ()> {
        self.rt.shutdown_now()
    }
}
