use crate::runtime;
use futures01::{future, stream::Stream, Async, Future};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use stream_cancel::{Trigger, Tripwire};
use tokio::timer;

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

    pub fn shutdown_source_begin(&mut self, name: &str) {
        self.shutdown_begun_triggers
            .remove(name)
            .expect(&format!(
                "shutdown_begun_trigger for source '{}' not found in the ShutdownCoordinator",
                name
            ))
            .cancel();
    }

    /// Waits for the source to shut down until the deadline.  If the source does not
    /// notify the shutdown_complete_tripwire for this source before the dealine, then signals
    /// the shutdown_force_trigger for this source to force it to shut down.  Returns whether
    /// or not the source shutdown gracefully.
    // TODO: The timing and reporting logic is very similar to the logic in
    // `RunningTopology::stop()`. Once `RunningTopology::stop()` has been updated to utilize the
    // ShutdownCoordinator, see if some of this logic can be de-duped.
    pub fn shutdown_source_end(
        &mut self,
        rt: &mut runtime::Runtime,
        name: String,
        deadline: Instant,
    ) -> bool {
        let name2 = name.clone();
        let name3 = name.clone();
        let shutdown_complete_tripwire =
            self.shutdown_complete_tripwires
                .remove(&name)
                .expect(&format!(
                "shutdown_complete_tripwire for source '{}' not found in the ShutdownCoordinator",
                name
            ));
        let shutdown_force_trigger = self.shutdown_force_triggers.remove(&name).expect(&format!(
            "shutdown_force_trigger for source '{}' not found in the ShutdownCoordinator",
            name
        ));

        let success = shutdown_complete_tripwire.map(move |_| {
            info!("Source \"{}\" shut down successfully", name);
        });
        let timeout = timer::Delay::new(deadline)
            .map(move |_| {
                error!(
                    "Source '{}' failed to shutdown before deadline. Forcing shutdown.",
                    name2,
                );
            })
            .map_err(|err| panic!("Timer error: {:?}", err));
        let reporter = timer::Interval::new_interval(Duration::from_secs(5))
            .inspect(move |_| {
                let time_remaining = if deadline > Instant::now() {
                    format!(
                        "{} seconds remaining",
                        (deadline - Instant::now()).as_secs()
                    )
                } else {
                    "overdue".to_string()
                };

                info!(
                    "Still waiting on source \"{}\" to shut down. {}",
                    name3, time_remaining,
                );
            })
            .filter(|_| false) // Run indefinitely without emitting items
            .into_future()
            .map(|_| ())
            .map_err(|(err, _)| panic!("Timer error: {:?}", err));

        let union = future::select_all::<Vec<Box<dyn Future<Item = (), Error = ()> + Send>>>(vec![
            Box::new(success),
            Box::new(timeout),
            Box::new(reporter),
        ]);

        // Now block until one of the futures resolves and use the index of the resolved future
        // to decide whether it was the success or timeout future that resolved first.
        let index = match rt.block_on(union) {
            Ok((_, index, _)) => index,
            Err((err, _, _)) => panic!(
                "Error from select_all future while waiting for source to shut down: {:?}",
                err
            ),
        };

        let success = if index == 0 {
            true
        } else if index == 1 {
            false
        } else {
            panic!(
                "Neither success nor timeout future finished.  Index finished: {}",
                index
            );
        };
        if success {
            shutdown_force_trigger.disable();
        } else {
            shutdown_force_trigger.cancel();
        }
        success
    }
}

#[cfg(test)]
mod test {
    use crate::runtime;
    use crate::shutdown::ShutdownCoordinator;
    use futures01::future::Future;
    use std::time::{Duration, Instant};

    #[test]
    fn shutdown_coordinator_shutdown_source_clean() {
        let mut rt = runtime::Runtime::new().unwrap();
        let mut shutdown = ShutdownCoordinator::new();
        let name = "test";

        let (shutdown_signal, _) = shutdown.register_source(name);

        shutdown.shutdown_source_begin(name);

        drop(shutdown_signal);

        let deadline = Instant::now() + Duration::from_secs(1);
        assert_eq!(
            true,
            shutdown.shutdown_source_end(&mut rt, name.to_string(), deadline)
        );
    }

    #[test]
    fn shutdown_coordinator_shutdown_source_force() {
        let mut rt = runtime::Runtime::new().unwrap();
        let mut shutdown = ShutdownCoordinator::new();
        let name = "test";

        let (_shutdown_signal, force_shutdown_tripwire) = shutdown.register_source(name);

        shutdown.shutdown_source_begin(name);

        // Since we never drop the ShutdownSignal the ShutdownCoordinator assumes the Source is
        // still running and must force shutdown.
        let deadline = Instant::now() + Duration::from_secs(1);
        assert_eq!(
            false,
            shutdown.shutdown_source_end(&mut rt, name.to_string(), deadline)
        );

        assert!(force_shutdown_tripwire.wait().is_ok());
    }
}
