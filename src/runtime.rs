use std::io;

use futures;
use futures::future::Future;


// TODO: I assume create a wrapper around this
use tokio::runtime::{Builder, Shutdown, TaskExecutor};

pub struct Runtime {
	rt: tokio::runtime::Runtime,
}

impl Runtime {
    pub fn new() -> io::Result<Self> {
        Ok(Runtime{ rt: tokio::runtime::Runtime::new()? })
    }

    pub fn single_threaded() -> io::Result<Self> {
        Self::with_thread_count(1)
    }

    pub fn with_thread_count(number: usize) -> io::Result<Self> {
        Ok(Runtime { rt: Builder::new().core_threads(number).build()? })
    }

    pub fn spawn<F>(&mut self, future: F) -> &mut Self
    where F: Future<Item=(), Error=()> + Send + 'static,
    {
        self.rt.spawn(future);
        self
    }

    pub fn executor(&self) -> TaskExecutor {
        self.rt.executor()
    }

    pub fn block_on<F, R, E>(&mut self, future: F) -> Result<R, E>
    where
        F: Send + 'static + Future<Item = R, Error = E>,
        R: Send + 'static,
        E: Send + 'static,
    {
        self.rt.block_on(future)
    }

    pub fn shutdown_on_idle(mut self) -> Shutdown {
        // Breaks wrapper here
        self.rt.shutdown_on_idle()
    }

    pub fn shutdown_now(mut self) -> Shutdown {
        self.rt.shutdown_now()
    }
}