use futures01::{Async, Future};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use stream_cancel::{Trigger, Tripwire};

/// When this struct goes out of scope and its internal refcount goes to 0 it is a signal that its
/// corresponding Source has completed executing and may be cleaned up.  It is the responsibility
/// of each Source to ensure that at least one copy of this handle remains alive for the entire
/// lifetime of the Source.
#[derive(Clone)]
pub struct ShutdownCompleteHandle {
    _shutdown_complete: Arc<Trigger>,
}

impl ShutdownCompleteHandle {
    fn new(shutdown_complete: Trigger) -> Self {
        Self {
            _shutdown_complete: Arc::new(shutdown_complete),
        }
    }
}

/// Passed to each Source to coordinate the global shutdown process.
#[derive(Clone)]
pub struct ShutdownSignal {
    /// This will be triggered when global shutdown has begun, and is a sign to the Source to begin
    /// its shutdown process.
    begin_shutdown: Tripwire,

    /// When a Source allows this to go out of scope it informs the global shutdown coordinator that
    /// this Source's local shutdown process is complete.
    /// Optional only so that `poll()` can move the handle out and return it.
    shutdown_complete: Option<ShutdownCompleteHandle>,
}

impl Future for ShutdownSignal {
    type Item = ShutdownCompleteHandle;
    type Error = ();
    fn poll(&mut self) -> Result<Async<Self::Item>, Self::Error> {
        match self.begin_shutdown.poll() {
            Ok(Async::Ready(_)) => Ok(Async::Ready(self.shutdown_complete.take().unwrap())),
            Ok(Async::NotReady) => Ok(Async::NotReady),
            Err(_) => Err(()),
        }
    }
}

impl ShutdownSignal {
    pub fn new(begin_shutdown: Tripwire, shutdown_complete: Trigger) -> Self {
        Self {
            begin_shutdown,
            shutdown_complete: Some(ShutdownCompleteHandle::new(shutdown_complete)),
        }
    }

    /// Only for testing.
    pub fn noop() -> Self {
        let (trigger, tripwire) = Tripwire::new();
        Self {
            begin_shutdown: tripwire,
            shutdown_complete: Some(ShutdownCompleteHandle::new(trigger)),
        }
    }
}

pub struct ShutdownCoordinator {
    shutdown_begun_triggers: HashMap<String, Trigger>,
    shutdown_force_triggers: HashMap<String, Trigger>,
    shutdown_complete_tripwires: HashMap<String, Tripwire>,
}

impl ShutdownCoordinator {
    pub fn new() -> Self {
        Self {
            shutdown_begun_triggers: HashMap::new(),
            shutdown_complete_tripwires: HashMap::new(),
            shutdown_force_triggers: HashMap::new(),
        }
    }

    /// Creates the necessary Triggers and Tripwires for coordinating shutdown of this Source and
    /// stores them as needed.  Returns the ShutdownSignal for this Source as well as a Tripwire
    /// that will be notified if the Source should be forcibly shut down.
    pub fn register_source(&mut self, name: &str) -> (ShutdownSignal, Tripwire) {
        let (shutdown_begun_trigger, shutdown_begun_tripwire) = Tripwire::new();
        let (force_shutdown_trigger, force_shutdown_tripwire) = Tripwire::new();
        let (shutdown_complete_trigger, shutdown_complete_tripwire) = Tripwire::new();

        self.shutdown_begun_triggers
            .insert(name.to_string(), shutdown_begun_trigger);
        self.shutdown_force_triggers
            .insert(name.to_string(), force_shutdown_trigger);
        self.shutdown_complete_tripwires
            .insert(name.to_string(), shutdown_complete_tripwire);

        let shutdown_signal =
            ShutdownSignal::new(shutdown_begun_tripwire, shutdown_complete_trigger);

        (shutdown_signal, force_shutdown_tripwire)
    }

    pub fn shutdown_source_begin(&mut self, name: &str) {
        self.shutdown_begun_triggers.remove(name).unwrap().cancel();
    }

    pub fn shutdown_source_end(&mut self, name: &str, deadline: &Instant) {
        let mut seconds_passed = 0;
        let mut shutdown_complete_tripwire = self.shutdown_complete_tripwires.remove(name).unwrap();
        while Instant::now() < *deadline {
            match shutdown_complete_tripwire.poll() {
                Ok(Async::Ready(_)) => break,
                Ok(Async::NotReady) => {
                    if seconds_passed % 5 == 0 {
                        // print message every 5 seconds
                        info!("Still waiting on source \"{}\" to shut down", name);
                    }
                    std::thread::sleep(Duration::from_secs(1));
                    seconds_passed += 1;
                    continue;
                }
                Err(_) => {
                    panic!("Got error waiting on Tripwire, this shouldn't be possible");
                }
            }
        }

        if Instant::now() >= *deadline {
            error!(
                "Source '{}' failed to shutdown before deadline. Forcing shutdown.",
                name
            );
            self.shutdown_force_triggers.remove(name).unwrap().cancel();
        } else {
            info!("Source \"{}\" shut down successfully", name);
            self.shutdown_force_triggers.remove(name).unwrap().disable();
        }
    }
}
