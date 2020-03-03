use std::sync::Arc;
use stream_cancel::{Trigger, Tripwire};

/// When this struct goes out of scope it's a signal that its corresponding Source has completed
/// executing and may be cleaned up.  It is the responsibility of each Source to ensure that this
/// handle remains alive for the entire lifetime of the Source.  Source implementations must call
/// take() on this at least once, and when multiple threads do work on behalf of a single Source
/// each thread should own a clone of this and call take() on it at least once.
pub struct SourceShutdownHandle {
    _shutdown_complete: Arc<Trigger>,
    taken: bool,
}

impl SourceShutdownHandle {
    fn new(shutdown_complete: Trigger) -> Self {
        Self {
            _shutdown_complete: Arc::new(shutdown_complete),
            taken: false,
        }
    }

    /// Used to force moving this struct into a closure.
    /// Every thread doing work on behalf of a Source must call this at least once.
    pub fn take(&mut self) {
        self.taken = true;
    }
}

impl Drop for SourceShutdownHandle {
    fn drop(&mut self) {
        panic!("SourceShutdownHandle dropped without ever being taken");
    }
}

/// Passed to each Source to coordinate the global shutdown process.
pub struct ShutdownSignals {
    /// This will be triggered when global shutdown has begun, and is a sign to the Source to begin
    /// its shutdown process.
    pub begin_shutdown: Tripwire,

    /// When a Source allows this to go out of scope it informs the global shutdown coordinator that
    /// this source's local shutdown process is complete.
    pub shutdown_complete: SourceShutdownHandle,
}

impl ShutdownSignals {
    pub fn new(begin_shutdown: Tripwire, shutdown_complete: Trigger) -> Self {
        Self {
            begin_shutdown,
            shutdown_complete: SourceShutdownHandle::new(shutdown_complete),
        }
    }
}
