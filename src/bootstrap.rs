#![allow(missing_docs)]

use tokio::{
    sync::watch,
    task::{JoinError, JoinHandle},
};

use crate::signal::{ShutdownState, wait_for_shutdown};

/// Marker for a phase being interrupted by a shutdown signal.
pub(crate) struct Interrupted;

/// Orchestrates startup/validation phases against shutdown signals.
///
/// Reload requests remain in their mailbox while a phase is blocked.
pub(crate) struct Bootstrap<'a> {
    shutdown: &'a mut watch::Receiver<ShutdownState>,
    guards: Vec<Box<dyn FnOnce()>>,
}

impl<'a> Bootstrap<'a> {
    pub(crate) fn new(shutdown: &'a mut watch::Receiver<ShutdownState>) -> Self {
        Self {
            shutdown,
            guards: Default::default(),
        }
    }

    /// Races a phase against durable shutdown state, including an earlier shutdown.
    pub(crate) async fn phase<T>(
        &mut self,
        fut: impl Future<Output = T>,
    ) -> Result<T, Interrupted> {
        tokio::select! {
            biased;
            _ = wait_for_shutdown(self.shutdown) => {
                self.run_guards();
                Err(Interrupted)
            }
            result = fut => self.complete_phase(result),
        }
    }

    /// Races a spawned phase's `JoinHandle` against shutdown; on interrupt the handle is aborted
    /// (not awaited, so `spawn_blocking` can't hang the shutdown).
    pub(crate) async fn phase_join<T>(
        &mut self,
        handle: &mut JoinHandle<T>,
    ) -> Result<Result<T, JoinError>, Interrupted> {
        tokio::select! {
            biased;
            // Shutdown arrived mid-phase: abort and report it. The process exits immediately
            // on this path, so detached blocking work dies with it.
            _ = wait_for_shutdown(self.shutdown) => {
                handle.abort();
                self.run_guards();
                Err(Interrupted)
            }
            result = &mut *handle => self.complete_phase(result),
        }
    }

    fn complete_phase<T>(&mut self, result: T) -> Result<T, Interrupted> {
        // The phase can observe shutdown after select polled its shutdown branch.
        if self.pending_shutdown() {
            self.run_guards();
            Err(Interrupted)
        } else {
            Ok(result)
        }
    }

    /// Registers a cleanup to run on interrupt or completion.
    pub(crate) fn guard(&mut self, cleanup: impl FnOnce() + 'static) {
        self.guards.push(Box::new(cleanup));
    }

    /// Runs all registered cleanups, draining the registry.
    pub(crate) fn run_guards(&mut self) {
        for guard in self.guards.drain(..) {
            guard();
        }
    }

    /// Checks shutdown state without consuming it.
    pub(crate) fn pending_shutdown(&self) -> bool {
        self.shutdown.borrow().is_shutdown() || self.shutdown.has_changed().is_err()
    }
}

#[cfg(test)]
mod tests {
    use std::{cell::Cell, rc::Rc};

    use super::*;
    use crate::signal::{ShutdownSignal, Signals};

    #[tokio::test]
    async fn shutdown_interrupts_successive_phases_and_runs_guards_once() {
        let mut signals = Signals::default();
        let mut bootstrap = Bootstrap::new(&mut signals.shutdown);
        let cleanups = Rc::new(Cell::new(0));
        let count = Rc::clone(&cleanups);
        bootstrap.guard(move || count.set(count.get() + 1));

        let interrupted = bootstrap
            .phase(async {
                signals.handler.shutdown.send(ShutdownSignal::Graceful);
                std::future::pending::<()>().await;
            })
            .await;
        assert!(interrupted.is_err());
        assert_eq!(cleanups.get(), 1);

        // An observed shutdown must still win against a ready phase.
        assert!(
            bootstrap
                .phase(async { panic!("phase ran after shutdown") })
                .await
                .is_err()
        );
        assert_eq!(cleanups.get(), 1);
    }

    #[tokio::test]
    async fn completed_phase_cannot_hide_concurrent_shutdown() {
        let mut signals = Signals::default();
        let mut bootstrap = Bootstrap::new(&mut signals.shutdown);
        let result = bootstrap
            .phase(async {
                signals.handler.shutdown.send(ShutdownSignal::Graceful);
            })
            .await;
        assert!(result.is_err());
    }
}
