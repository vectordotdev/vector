use futures::{Async, Future};
use std::sync::Arc;
use stream_cancel::{Trigger, Tripwire};

/// When this struct goes out of scope it's a signal that its corresponding Source has completed
/// executing and may be cleaned up.  It is the responsibility of each Source to ensure that this
/// handle remains alive for the entire lifetime of the Source.  Source implementations must call
/// take() on this at least once, and when multiple threads do work on behalf of a single Source
/// each thread should own a clone of this and call take() on it at least once.
#[derive(Clone)]
pub struct SourceShutdownHandle {
    _shutdown_complete: Arc<Trigger>,
}

impl SourceShutdownHandle {
    fn new(shutdown_complete: Trigger) -> Self {
        Self {
            _shutdown_complete: Arc::new(shutdown_complete),
        }
    }

    /// No-op used to force moving this struct into a closure.
    pub fn take(&self) {}
}

/// Passed to each Source to coordinate the global shutdown process.
pub struct ShutdownSignals {
    /// This will be triggered when global shutdown has begun, and is a sign to the Source to begin
    /// its shutdown process.
    begin_shutdown: Option<Tripwire>,

    /// When a Source allows this to go out of scope it informs the global shutdown coordinator that
    /// this source's local shutdown process is complete.
    shutdown_complete: Option<SourceShutdownHandle>,
}

impl Future for ShutdownSignals {
    type Item = Option<SourceShutdownHandle>;
    type Error = ();
    fn poll(&mut self) -> Result<Async<Self::Item>, Self::Error> {
        match self.begin_shutdown.poll() {
            Ok(Async::Ready(_)) => Ok(Async::Ready(self.shutdown_complete.take())),
            Ok(Async::NotReady) => Ok(Async::NotReady),
            Err(_) => Err(()),
        }
    }
}

impl ShutdownSignals {
    pub fn new(begin_shutdown: Tripwire, shutdown_complete: Trigger) -> Self {
        Self {
            begin_shutdown: Some(begin_shutdown),
            shutdown_complete: Some(SourceShutdownHandle::new(shutdown_complete)),
        }
    }

    pub fn noop() -> Self {
        Self {
            begin_shutdown: None,
            shutdown_complete: None,
        }
    }

    /// TODO comment.  Note illegal to call after Future::poll()
    pub fn get_shutdown_complete_handle(&self) -> SourceShutdownHandle {
        self.shutdown_complete.as_ref().unwrap().clone()
    }
}
