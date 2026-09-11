#![allow(missing_docs)]

use std::{collections::HashSet, future::Future};

use tokio::task::JoinError;
use tokio::task::JoinHandle;

use crate::config::{ComponentKey, ConfigBuilder};
use crate::signal::{
    ShutdownReceiver, SignalRx, SignalTo, SignalTx, recv_shutdown, try_recv_shutdown,
};

/// Marker for a phase being interrupted by a shutdown signal.
pub(crate) struct Interrupted;

/// Coalesced reload signals received while a phase is blocked, so a burst of reloads during a
/// long outage is bounded (only the latest state per kind is retained) and replayed in a stable
/// order.
#[derive(Default)]
struct PendingReloads {
    from_disk: bool,
    components: Option<HashSet<ComponentKey>>,
    from_builder: Option<ConfigBuilder>,
    enrichment_tables: bool,
}

impl PendingReloads {
    fn push(&mut self, signal: SignalTo) {
        match signal {
            SignalTo::ReloadFromDisk => self.from_disk = true,
            SignalTo::ReloadComponents(components) => {
                self.components
                    .get_or_insert_with(HashSet::new)
                    .extend(components);
            }
            SignalTo::ReloadFromConfigBuilder(builder) => self.from_builder = Some(builder),
            SignalTo::ReloadEnrichmentTables => self.enrichment_tables = true,
        }
    }

    fn into_signals(self) -> Vec<SignalTo> {
        let mut signals = Vec::new();
        if self.from_disk {
            signals.push(SignalTo::ReloadFromDisk);
        }
        if let Some(components) = self.components {
            signals.push(SignalTo::ReloadComponents(components));
        }
        if let Some(builder) = self.from_builder {
            signals.push(SignalTo::ReloadFromConfigBuilder(builder));
        }
        if self.enrichment_tables {
            signals.push(SignalTo::ReloadEnrichmentTables);
        }
        signals
    }
}

/// Orchestrates startup/validation phases against shutdown signals.
///
/// Every phase runs through [`Bootstrap::phase`] (or [`Bootstrap::phase_join`] for a spawned
/// task), which races the phase against `recv_shutdown` with the signal branch biased first, so
/// a phase cannot run unraced by construction. Reload signals received along the way are
/// coalesced into a `PendingReloads` set and re-broadcast via `signal_tx` by
/// [`Bootstrap::replay_reloads`]; shutdown signals arrive on a dedicated channel, so a burst
/// of reloads can never overflow and drop one; cleanups registered via [`Bootstrap::guard`]
/// run on interrupt or completion.
pub(crate) struct Bootstrap<'a> {
    signal_rx: &'a mut SignalRx,
    shutdown_rx: &'a mut ShutdownReceiver,
    signal_tx: SignalTx,
    pending_reloads: PendingReloads,
    guards: Vec<Box<dyn FnOnce()>>,
}

impl<'a> Bootstrap<'a> {
    pub(crate) fn new(
        signal_rx: &'a mut SignalRx,
        shutdown_rx: &'a mut ShutdownReceiver,
        signal_tx: SignalTx,
    ) -> Self {
        Self {
            signal_rx,
            shutdown_rx,
            signal_tx,
            pending_reloads: Default::default(),
            guards: Default::default(),
        }
    }

    /// Runs a phase, racing it against shutdown signals (biased, signal first). Returns
    /// `Err(Interrupted)` if a shutdown signal (or a closed channel) arrives before the phase
    /// completes; reload signals received along the way are coalesced for later replay.
    pub(crate) async fn phase<T>(
        &mut self,
        fut: impl Future<Output = T>,
    ) -> Result<T, Interrupted> {
        let mut fut = Box::pin(fut);
        tokio::select! {
            biased;
            // A shutdown signal (or a closed signal channel) arrived while the phase was still
            // in progress. Reload signals received along the way are retained by the sink so
            // they can be re-broadcast once the phase completes.
            _ = recv_shutdown(self.signal_rx, self.shutdown_rx, |reload| self.pending_reloads.push(reload)) => {
                self.run_guards();
                Err(Interrupted)
            }
            result = &mut fut => Ok(result),
        }
    }

    /// Runs a spawned phase, racing its `JoinHandle` against shutdown signals. On interrupt the
    /// handle is aborted (not awaited, so a `spawn_blocking` phase cannot hang the shutdown)
    /// and `Err(Interrupted)` is returned; otherwise the handle's output is returned.
    pub(crate) async fn phase_join<T>(
        &mut self,
        handle: &mut JoinHandle<T>,
    ) -> Result<Result<T, JoinError>, Interrupted> {
        tokio::select! {
            biased;
            // A shutdown signal (or a closed signal channel) arrived while the spawned phase
            // was still running. Cancel it and report the interrupt; the process exits
            // immediately on this path, so any detached blocking work dies with it.
            _ = recv_shutdown(self.signal_rx, self.shutdown_rx, |reload| self.pending_reloads.push(reload)) => {
                handle.abort();
                self.run_guards();
                Err(Interrupted)
            }
            result = &mut *handle => Ok(result),
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

    /// Re-broadcasts coalesced reload signals collected since the last replay, in a stable order
    /// (disk, components, builder, enrichment).
    pub(crate) fn replay_reloads(&mut self) {
        for reload in std::mem::take(&mut self.pending_reloads).into_signals() {
            drop(self.signal_tx.send(reload));
        }
    }

    /// Non-blocking check for a queued shutdown signal, coalescing any reload signals into the
    /// pending set.
    pub(crate) fn pending_shutdown(&mut self) -> bool {
        try_recv_shutdown(self.signal_rx, self.shutdown_rx, |reload| {
            self.pending_reloads.push(reload)
        })
        .is_some()
    }
}
