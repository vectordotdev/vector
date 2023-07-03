#![allow(clippy::module_name_repetitions)]

use std::{
    collections::HashMap,
    future::Future,
    pin::Pin,
    sync::Arc,
    task::{ready, Context, Poll},
};

use futures::{future, FutureExt};
use stream_cancel::{Trigger, Tripwire};
use tokio::time::{timeout_at, Instant};

use crate::{config::ComponentKey, trigger::DisabledTrigger};

pub async fn tripwire_handler(closed: bool) {
    std::future::poll_fn(|_| {
        if closed {
            Poll::Ready(())
        } else {
            Poll::Pending
        }
    })
    .await;
}

/// When this struct goes out of scope and its internal refcount goes to 0 it is a signal that its
/// corresponding `Source` has completed executing and may be cleaned up.  It is the responsibility
/// of each `Source` to ensure that at least one copy of this handle remains alive for the entire
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

/// Passed to each `Source` to coordinate the global shutdown process.
#[pin_project::pin_project]
#[derive(Clone, Debug)]
pub struct ShutdownSignal {
    /// This will be triggered when global shutdown has begun, and is a sign to the Source to begin
    /// its shutdown process.
    #[pin]
    begin_shutdown: Option<Tripwire>,

    /// When a Source allows this to go out of scope it informs the global shutdown coordinator that
    /// this Source's local shutdown process is complete.
    /// Optional only so that `poll()` can move the handle out and return it.
    shutdown_complete: Option<ShutdownSignalToken>,
}

impl Future for ShutdownSignal {
    type Output = ShutdownSignalToken;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match self.as_mut().project().begin_shutdown.as_pin_mut() {
            Some(fut) => {
                let closed = ready!(fut.poll(cx));
                let mut pinned = self.project();
                pinned.begin_shutdown.set(None);
                if closed {
                    Poll::Ready(pinned.shutdown_complete.take().unwrap())
                } else {
                    Poll::Pending
                }
            }
            // TODO: This should almost certainly be a panic to avoid deadlocking in the case of a
            // poll-after-ready situation.
            None => Poll::Pending,
        }
    }
}

impl ShutdownSignal {
    #[must_use]
    pub fn new(tripwire: Tripwire, trigger: Trigger) -> Self {
        Self {
            begin_shutdown: Some(tripwire),
            shutdown_complete: Some(ShutdownSignalToken::new(trigger)),
        }
    }

    #[must_use]
    pub fn noop() -> Self {
        let (trigger, tripwire) = Tripwire::new();
        Self {
            begin_shutdown: Some(tripwire),
            shutdown_complete: Some(ShutdownSignalToken::new(trigger)),
        }
    }

    #[must_use]
    pub fn new_wired() -> (Trigger, ShutdownSignal, Tripwire) {
        let (trigger_shutdown, tripwire) = Tripwire::new();
        let (trigger, shutdown_done) = Tripwire::new();
        let shutdown = ShutdownSignal::new(tripwire, trigger);

        (trigger_shutdown, shutdown, shutdown_done)
    }
}

type IsInternal = bool;

#[derive(Debug, Default)]
pub struct SourceShutdownCoordinator {
    shutdown_begun_triggers: HashMap<ComponentKey, (IsInternal, Trigger)>,
    shutdown_force_triggers: HashMap<ComponentKey, Trigger>,
    shutdown_complete_tripwires: HashMap<ComponentKey, Tripwire>,
}

impl SourceShutdownCoordinator {
    /// Creates the necessary Triggers and Tripwires for coordinating shutdown of this Source and
    /// stores them as needed.  Returns the `ShutdownSignal` for this Source as well as a Tripwire
    /// that will be notified if the Source should be forcibly shut down.
    pub fn register_source(
        &mut self,
        id: &ComponentKey,
        internal: bool,
    ) -> (ShutdownSignal, impl Future<Output = ()>) {
        let (shutdown_begun_trigger, shutdown_begun_tripwire) = Tripwire::new();
        let (force_shutdown_trigger, force_shutdown_tripwire) = Tripwire::new();
        let (shutdown_complete_trigger, shutdown_complete_tripwire) = Tripwire::new();

        self.shutdown_begun_triggers
            .insert(id.clone(), (internal, shutdown_begun_trigger));
        self.shutdown_force_triggers
            .insert(id.clone(), force_shutdown_trigger);
        self.shutdown_complete_tripwires
            .insert(id.clone(), shutdown_complete_tripwire);

        let shutdown_signal =
            ShutdownSignal::new(shutdown_begun_tripwire, shutdown_complete_trigger);

        // `force_shutdown_tripwire` resolves even if canceled when we should *not* be shutting down.
        // `tripwire_handler` handles cancel by never resolving.
        let force_shutdown_tripwire = force_shutdown_tripwire.then(tripwire_handler);
        (shutdown_signal, force_shutdown_tripwire)
    }

    /// Takes ownership of all internal state for the given source from another `ShutdownCoordinator`.
    ///
    /// # Panics
    ///
    /// Panics if the other coordinator already had its triggers removed.
    pub fn takeover_source(&mut self, id: &ComponentKey, other: &mut Self) {
        let existing = self.shutdown_begun_triggers.insert(
            id.clone(),
            other.shutdown_begun_triggers.remove(id).unwrap_or_else(|| {
                panic!(
                    "Other ShutdownCoordinator didn't have a shutdown_begun_trigger for \"{id}\""
                )
            }),
        );
        assert!(
            existing.is_none(),
            "ShutdownCoordinator already has a shutdown_begin_trigger for source \"{id}\""
        );

        let existing = self.shutdown_force_triggers.insert(
            id.clone(),
            other.shutdown_force_triggers.remove(id).unwrap_or_else(|| {
                panic!(
                    "Other ShutdownCoordinator didn't have a shutdown_force_trigger for \"{id}\""
                )
            }),
        );
        assert!(
            existing.is_none(),
            "ShutdownCoordinator already has a shutdown_force_trigger for source \"{id}\""
        );

        let existing = self.shutdown_complete_tripwires.insert(
            id.clone(),
            other
                .shutdown_complete_tripwires
                .remove(id)
                .unwrap_or_else(|| {
                    panic!(
                        "Other ShutdownCoordinator didn't have a shutdown_complete_tripwire for \"{id}\""
                    )
                }),
        );
        assert!(
            existing.is_none(),
            "ShutdownCoordinator already has a shutdown_complete_tripwire for source \"{id}\""
        );
    }

    /// Sends a signal to begin shutting down to all sources, and returns a future that
    /// resolves once all sources have either shut down completely, or have been sent the
    /// force shutdown signal.  The force shutdown signal will be sent to any sources that
    /// don't cleanly shut down before the given `deadline`.
    ///
    /// # Panics
    ///
    /// Panics if this coordinator has had its triggers removed (ie
    /// has been taken over with `Self::takeover_source`).
    pub fn shutdown_all(self, deadline: Option<Instant>) -> impl Future<Output = ()> {
        let mut internal_sources_complete_futures = Vec::new();
        let mut external_sources_complete_futures = Vec::new();

        let shutdown_begun_triggers = self.shutdown_begun_triggers;
        let mut shutdown_complete_tripwires = self.shutdown_complete_tripwires;
        let mut shutdown_force_triggers = self.shutdown_force_triggers;

        for (id, (internal, trigger)) in shutdown_begun_triggers {
            trigger.cancel();

            let shutdown_complete_tripwire =
                shutdown_complete_tripwires.remove(&id).unwrap_or_else(|| {
                    panic!(
                "shutdown_complete_tripwire for source \"{id}\" not found in the ShutdownCoordinator"
            )
                });
            let shutdown_force_trigger = shutdown_force_triggers.remove(&id).unwrap_or_else(|| {
                panic!(
                    "shutdown_force_trigger for source \"{id}\" not found in the ShutdownCoordinator"
                )
            });

            let source_complete = SourceShutdownCoordinator::shutdown_source_complete(
                shutdown_complete_tripwire,
                shutdown_force_trigger,
                id.clone(),
                deadline,
            );

            if internal {
                internal_sources_complete_futures.push(source_complete);
            } else {
                external_sources_complete_futures.push(source_complete);
            }
        }

        futures::future::join_all(external_sources_complete_futures)
            .then(|_| futures::future::join_all(internal_sources_complete_futures))
            .map(|_| ())
    }

    /// Sends the signal to the given source to begin shutting down. Returns a future that resolves
    /// when the source has finished shutting down cleanly or been sent the force shutdown signal.
    /// The returned future resolves to a bool that indicates if the source shut down cleanly before
    /// the given `deadline`. If the result is false then that means the source failed to shut down
    /// before `deadline` and had to be force-shutdown.
    ///
    /// # Panics
    ///
    /// Panics if this coordinator has had its triggers removed (ie
    /// has been taken over with `Self::takeover_source`).
    pub fn shutdown_source(
        &mut self,
        id: &ComponentKey,
        deadline: Instant,
    ) -> impl Future<Output = bool> {
        let (_, begin_shutdown_trigger) =
            self.shutdown_begun_triggers.remove(id).unwrap_or_else(|| {
                panic!(
                "shutdown_begun_trigger for source \"{id}\" not found in the ShutdownCoordinator"
            )
            });
        // This is what actually triggers the source to begin shutting down.
        begin_shutdown_trigger.cancel();

        let shutdown_complete_tripwire = self
            .shutdown_complete_tripwires
            .remove(id)
            .unwrap_or_else(|| {
                panic!(
                "shutdown_complete_tripwire for source \"{id}\" not found in the ShutdownCoordinator"
            )
            });
        let shutdown_force_trigger = self.shutdown_force_triggers.remove(id).unwrap_or_else(|| {
            panic!(
                "shutdown_force_trigger for source \"{id}\" not found in the ShutdownCoordinator"
            )
        });
        SourceShutdownCoordinator::shutdown_source_complete(
            shutdown_complete_tripwire,
            shutdown_force_trigger,
            id.clone(),
            Some(deadline),
        )
    }

    /// Returned future will finish once all *current* sources have finished.
    #[must_use]
    pub fn shutdown_tripwire(&self) -> future::BoxFuture<'static, ()> {
        let futures = self
            .shutdown_complete_tripwires
            .values()
            .cloned()
            .map(|tripwire| tripwire.then(tripwire_handler).boxed());

        future::join_all(futures)
            .map(|_| info!("All sources have finished."))
            .boxed()
    }

    fn shutdown_source_complete(
        shutdown_complete_tripwire: Tripwire,
        shutdown_force_trigger: Trigger,
        id: ComponentKey,
        deadline: Option<Instant>,
    ) -> impl Future<Output = bool> {
        async move {
            let fut = shutdown_complete_tripwire.then(tripwire_handler);
            if let Some(deadline) = deadline {
                // Call `shutdown_force_trigger.disable()` on drop.
                let shutdown_force_trigger = DisabledTrigger::new(shutdown_force_trigger);
                if timeout_at(deadline, fut).await.is_ok() {
                    shutdown_force_trigger.into_inner().disable();
                    true
                } else {
                    error!(
                        "Source '{}' failed to shutdown before deadline. Forcing shutdown.",
                        id,
                    );
                    shutdown_force_trigger.into_inner().cancel();
                    false
                }
            } else {
                fut.await;
                true
            }
        }
        .boxed()
    }
}

#[cfg(test)]
mod test {
    use tokio::time::{Duration, Instant};

    use super::*;
    use crate::shutdown::SourceShutdownCoordinator;

    #[tokio::test]
    async fn shutdown_coordinator_shutdown_source_clean() {
        let mut shutdown = SourceShutdownCoordinator::default();
        let id = ComponentKey::from("test");

        let (shutdown_signal, _) = shutdown.register_source(&id, false);

        let deadline = Instant::now() + Duration::from_secs(1);
        let shutdown_complete = shutdown.shutdown_source(&id, deadline);

        drop(shutdown_signal);

        let success = shutdown_complete.await;
        assert!(success);
    }

    #[tokio::test]
    async fn shutdown_coordinator_shutdown_source_force() {
        let mut shutdown = SourceShutdownCoordinator::default();
        let id = ComponentKey::from("test");

        let (_shutdown_signal, force_shutdown_tripwire) = shutdown.register_source(&id, false);

        let deadline = Instant::now() + Duration::from_secs(1);
        let shutdown_complete = shutdown.shutdown_source(&id, deadline);

        // Since we never drop the `ShutdownSignal` the `ShutdownCoordinator` assumes the Source is
        // still running and must force shutdown.
        let success = shutdown_complete.await;
        assert!(!success);

        let finished = futures::poll!(force_shutdown_tripwire.boxed());
        assert_eq!(finished, Poll::Ready(()));
    }
}
