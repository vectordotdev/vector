use futures01::{future, Async, Future};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use stream_cancel::{Trigger, Tripwire};
use tokio01::timer;

/// When this struct goes out of scope and its internal refcount goes to 0 it is a signal that its
/// corresponding Source has completed executing and may be cleaned up.  It is the responsibility
/// of each Source to ensure that at least one copy of this handle remains alive for the entire
/// lifetime of the Source.
#[derive(Clone, Debug)]
pub struct ShutdownSignalToken {
    _shutdown_complete: Arc<Trigger>,
}

impl ShutdownSignalToken {
    fn new(shutdown_complete: Trigger) -> Self {
        Self {
            _shutdown_complete: Arc::new(shutdown_complete),
        }
    }
}

/// Passed to each Source to coordinate the global shutdown process.
#[derive(Clone, Debug)]
pub struct ShutdownSignal {
    /// This will be triggered when global shutdown has begun, and is a sign to the Source to begin
    /// its shutdown process.
    begin_shutdown: Tripwire,

    /// When a Source allows this to go out of scope it informs the global shutdown coordinator that
    /// this Source's local shutdown process is complete.
    /// Optional only so that `poll()` can move the handle out and return it.
    shutdown_complete: Option<ShutdownSignalToken>,
}

impl Future for ShutdownSignal {
    type Item = ShutdownSignalToken;
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
            shutdown_complete: Some(ShutdownSignalToken::new(shutdown_complete)),
        }
    }

    #[cfg(test)]
    pub fn noop() -> Self {
        let (trigger, tripwire) = Tripwire::new();
        Self {
            begin_shutdown: tripwire,
            shutdown_complete: Some(ShutdownSignalToken::new(trigger)),
        }
    }

    #[cfg(test)]
    pub fn new_wired() -> (Trigger, ShutdownSignal, Tripwire) {
        let (trigger_shutdown, tripwire) = Tripwire::new();
        let (trigger, shutdown_done) = Tripwire::new();
        let shutdown = ShutdownSignal::new(tripwire, trigger);

        (trigger_shutdown, shutdown, shutdown_done)
    }
}

pub struct SourceShutdownCoordinator {
    shutdown_begun_triggers: HashMap<String, Trigger>,
    shutdown_force_triggers: HashMap<String, Trigger>,
    shutdown_complete_tripwires: HashMap<String, Tripwire>,
}

impl SourceShutdownCoordinator {
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
    pub fn register_source(
        &mut self,
        name: &str,
    ) -> (ShutdownSignal, impl Future<Item = (), Error = ()>) {
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

        // shutdown_source_end drops the force_shutdown_trigger even on success when we should *not*
        // be shutting down.  Dropping the trigger will cause the Tripwire to resolve with an error,
        // so we use or_else with future::empty() to make it so it never resolves if the Trigger is
        // prematurely dropped instead.
        let force_shutdown_tripwire = force_shutdown_tripwire.or_else(|_| future::empty());
        (shutdown_signal, force_shutdown_tripwire)
    }

    /// Takes ownership of all internal state for the given source from another ShutdownCoordinator.
    pub fn takeover_source(&mut self, name: &str, other: &mut Self) {
        let existing = self.shutdown_begun_triggers.insert(
            name.to_string(),
            other.shutdown_begun_triggers.remove(name).expect(&format!(
                "Other ShutdownCoordinator didn't have a shutdown_begun_trigger for {}",
                name
            )),
        );
        if !existing.is_none() {
            panic!(
                "ShutdownCoordinator already has a shutdown_begin_trigger for source {}",
                name
            );
        }

        let existing = self.shutdown_force_triggers.insert(
            name.to_string(),
            other.shutdown_force_triggers.remove(name).expect(&format!(
                "Other ShutdownCoordinator didn't have a shutdown_force_trigger for {}",
                name
            )),
        );
        if !existing.is_none() {
            panic!(
                "ShutdownCoordinator already has a shutdown_force_trigger for source {}",
                name
            );
        }

        let existing = self.shutdown_complete_tripwires.insert(
            name.to_string(),
            other
                .shutdown_complete_tripwires
                .remove(name)
                .expect(&format!(
                    "Other ShutdownCoordinator didn't have a shutdown_complete_tripwire for {}",
                    name
                )),
        );
        if !existing.is_none() {
            panic!(
                "ShutdownCoordinator already has a shutdown_complete_tripwire for source {}",
                name
            );
        }
    }

    /// Sends a signal to begin shutting down to all sources, and returns a future that
    /// resolves once all sources have either shut down completely, or have been sent the
    /// force shutdown signal.  The force shutdown signal will be sent to any sources that
    /// don't cleanly shut down before the given `deadline`.
    pub fn shutdown_all(self, deadline: Instant) -> impl Future<Item = (), Error = ()> {
        let mut complete_futures = Vec::new();

        let shutdown_begun_triggers = self.shutdown_begun_triggers;
        let mut shutdown_complete_tripwires = self.shutdown_complete_tripwires;
        let mut shutdown_force_triggers = self.shutdown_force_triggers;

        for (name, trigger) in shutdown_begun_triggers {
            trigger.cancel();

            let shutdown_complete_tripwire =
                shutdown_complete_tripwires.remove(&name).expect(&format!(
                "shutdown_complete_tripwire for source '{}' not found in the ShutdownCoordinator",
                name
            ));
            let shutdown_force_trigger = shutdown_force_triggers.remove(&name).expect(&format!(
                "shutdown_force_trigger for source '{}' not found in the ShutdownCoordinator",
                name
            ));

            let source_complete = SourceShutdownCoordinator::shutdown_source_complete(
                shutdown_complete_tripwire,
                shutdown_force_trigger,
                name,
                deadline,
            );

            complete_futures.push(source_complete);
        }

        future::join_all(complete_futures)
            .map(|_| ())
            .map_err(|_| ())
    }

    /// Sends the signal to the given source to begin shutting down. Returns a future that resolves
    /// when the source has finished shutting down cleanly or been sent the force shutdown signal.
    /// The returned future resolves to a bool that indicates if the source shut down cleanly before
    /// the given `deadline`. If the result is false then that means the source failed to shut down
    /// before `deadline` and had to be force-shutdown.
    pub fn shutdown_source(
        &mut self,
        name: &str,
        deadline: Instant,
    ) -> impl Future<Item = bool, Error = ()> {
        let begin_shutdown_trigger = self.shutdown_begun_triggers.remove(name).expect(&format!(
            "shutdown_begun_trigger for source '{}' not found in the ShutdownCoordinator",
            name
        ));
        // This is what actually triggers the source to begin shutting down.
        begin_shutdown_trigger.cancel();

        let shutdown_complete_tripwire =
            self.shutdown_complete_tripwires
                .remove(name)
                .expect(&format!(
                "shutdown_complete_tripwire for source '{}' not found in the ShutdownCoordinator",
                name
            ));
        let shutdown_force_trigger = self.shutdown_force_triggers.remove(name).expect(&format!(
            "shutdown_force_trigger for source '{}' not found in the ShutdownCoordinator",
            name
        ));
        SourceShutdownCoordinator::shutdown_source_complete(
            shutdown_complete_tripwire,
            shutdown_force_trigger,
            name.to_owned(),
            deadline,
        )
    }

    /// Returned future will finish once all sources have finished.
    pub fn shutdown_tripwire(&self) -> impl Future<Item = (), Error = ()> {
        future::join_all(
            self.shutdown_complete_tripwires
                .values()
                .cloned()
                .collect::<Vec<_>>(),
        )
        .map(|_| info!("All sources have finished."))
    }

    fn shutdown_source_complete(
        shutdown_complete_tripwire: Tripwire,
        shutdown_force_trigger: Trigger,
        name: String,
        deadline: Instant,
    ) -> impl Future<Item = bool, Error = ()> {
        let success = shutdown_complete_tripwire.map(move |_| true);

        let timeout = timer::Delay::new(deadline)
            .map(move |_| {
                error!(
                    "Source '{}' failed to shutdown before deadline. Forcing shutdown.",
                    name,
                );
                false
            })
            .map_err(|err| panic!("Timer error: {:?}", err));

        let union = success.select(timeout);
        union
            .map(|(success, _)| {
                if success {
                    shutdown_force_trigger.disable();
                } else {
                    shutdown_force_trigger.cancel();
                }
                success
            })
            .map_err(|_| ())
    }
}

#[cfg(test)]
mod test {
    use crate::shutdown::SourceShutdownCoordinator;
    use crate::test_util::runtime;
    use futures01::future::Future;
    use std::time::{Duration, Instant};

    #[test]
    fn shutdown_coordinator_shutdown_source_clean() {
        let mut rt = runtime();
        let mut shutdown = SourceShutdownCoordinator::new();
        let name = "test";

        let (shutdown_signal, _) = shutdown.register_source(name);

        let deadline = Instant::now() + Duration::from_secs(1);
        let shutdown_complete = shutdown.shutdown_source(name, deadline);

        drop(shutdown_signal);

        let success = rt.block_on(shutdown_complete).unwrap();
        assert_eq!(true, success);
    }

    #[test]
    fn shutdown_coordinator_shutdown_source_force() {
        let mut rt = runtime();
        let mut shutdown = SourceShutdownCoordinator::new();
        let name = "test";

        let (_shutdown_signal, force_shutdown_tripwire) = shutdown.register_source(name);

        let deadline = Instant::now() + Duration::from_secs(1);
        let shutdown_complete = shutdown.shutdown_source(name, deadline);

        // Since we never drop the ShutdownSignal the ShutdownCoordinator assumes the Source is
        // still running and must force shutdown.
        let success = rt.block_on(shutdown_complete).unwrap();
        assert_eq!(false, success);
        assert!(force_shutdown_tripwire.wait().is_ok());
    }
}
