#![allow(missing_docs)]

use std::{
    collections::HashSet,
    sync::{Arc, Mutex, Weak},
    time::Duration,
};

use snafu::Snafu;
use tokio::{
    runtime::Runtime,
    sync::{Notify, broadcast, mpsc::error::SendError, watch},
    time::{Instant, sleep_until},
};
use tokio_stream::{Stream, StreamExt};

use super::config::{ComponentKey, ConfigBuilder};

const RELOAD_QUIET_PERIOD: Duration = Duration::from_millis(100);

/// Requests that contribute independent work to a reload plan.
#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
pub enum ReloadSignal {
    Components(HashSet<ComponentKey>),
    ConfigBuilder(ConfigBuilder),
    Disk,
    EnrichmentTables,
}

#[derive(Debug)]
pub enum ReloadConfig {
    Disk,
    Builder(Box<ConfigBuilder>),
}

/// One topology reload, plus any independent enrichment-table work.
#[derive(Debug, Default)]
pub struct ReloadPlan {
    pub config: Option<ReloadConfig>,
    pub components: HashSet<ComponentKey>,
    pub enrichment_tables: bool,
    pub reload_external_files: bool,
}

impl ReloadPlan {
    fn merge(&mut self, signal: ReloadSignal) {
        match signal {
            ReloadSignal::Components(components) => self.components.extend(components),
            ReloadSignal::ConfigBuilder(builder) => {
                self.config = Some(ReloadConfig::Builder(Box::new(builder)));
            }
            ReloadSignal::Disk => {
                self.config = Some(ReloadConfig::Disk);
                // A later builder replaces the configuration source, not the request
                // to reload transforms that read external files.
                self.reload_external_files = true;
            }
            ReloadSignal::EnrichmentTables => self.enrichment_tables = true,
        }
    }
}

#[derive(Default)]
struct ReloadMailbox {
    pending: Mutex<Option<(ReloadPlan, Instant)>>,
    changed: Notify,
}

/// Producers merge requests even while startup or an earlier reload is blocked.
#[derive(Clone)]
pub struct ReloadSender {
    mailbox: Weak<ReloadMailbox>,
}

impl ReloadSender {
    pub fn send(&self, signal: ReloadSignal) -> Result<(), Box<SendError<ReloadSignal>>> {
        let Some(mailbox) = self.mailbox.upgrade() else {
            return Err(Box::new(SendError(signal)));
        };
        {
            let mut pending = mailbox.pending.lock().expect("reload mailbox poisoned");
            let deadline = Instant::now() + RELOAD_QUIET_PERIOD;
            let (plan, quiet_until) =
                pending.get_or_insert_with(|| (ReloadPlan::default(), deadline));
            plan.merge(signal);
            *quiet_until = deadline;
        }
        mailbox.changed.notify_one();
        Ok(())
    }
}

/// The single consumer owns the mailbox. Receiving a plan is cancellation-safe.
pub struct ReloadReceiver {
    mailbox: Arc<ReloadMailbox>,
}

impl ReloadReceiver {
    pub async fn recv(&mut self) -> ReloadPlan {
        loop {
            let deadline = {
                let mut pending = self
                    .mailbox
                    .pending
                    .lock()
                    .expect("reload mailbox poisoned");
                match pending.as_ref() {
                    Some((_, deadline)) if *deadline <= Instant::now() => {
                        // Take only when returning: cancellation cannot lose pending work.
                        return pending.take().expect("pending reload disappeared").0;
                    }
                    Some((_, deadline)) => Some(*deadline),
                    None => None,
                }
            };
            if let Some(deadline) = deadline {
                tokio::select! {
                    _ = self.mailbox.changed.notified() => {},
                    _ = sleep_until(deadline) => {},
                }
            } else {
                self.mailbox.changed.notified().await;
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ShutdownSignal {
    Graceful,
    Failed(ShutdownError),
    Immediate,
}

/// Durable progress: running -> graceful/failed -> immediate, never backwards.
/// The first failure is retained when a later request forces immediate shutdown.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum ShutdownState {
    #[default]
    Running,
    Graceful,
    Failed(ShutdownError),
    Immediate {
        error: Option<ShutdownError>,
    },
}

impl ShutdownState {
    pub const fn is_shutdown(&self) -> bool {
        !matches!(self, Self::Running)
    }

    pub const fn is_immediate(&self) -> bool {
        matches!(self, Self::Immediate { .. })
    }

    pub const fn error(&self) -> Option<&ShutdownError> {
        match self {
            Self::Failed(error) | Self::Immediate { error: Some(error) } => Some(error),
            _ => None,
        }
    }
}

#[derive(Clone)]
pub struct ShutdownSender {
    tx: watch::Sender<ShutdownState>,
}

impl ShutdownSender {
    pub fn send(&self, signal: ShutdownSignal) {
        self.tx.send_modify(|state| {
            let previous = std::mem::take(state);
            *state = if previous.is_shutdown() {
                let previous_error = match previous {
                    ShutdownState::Failed(error) => Some(error),
                    ShutdownState::Immediate { error } => error,
                    _ => None,
                };
                ShutdownState::Immediate {
                    error: previous_error.or(match signal {
                        ShutdownSignal::Failed(error) => Some(error),
                        _ => None,
                    }),
                }
            } else {
                match signal {
                    ShutdownSignal::Graceful => ShutdownState::Graceful,
                    ShutdownSignal::Failed(error) => ShutdownState::Failed(error),
                    ShutdownSignal::Immediate => ShutdownState::Immediate { error: None },
                }
            };
        });
    }

    pub fn subscribe(&self) -> watch::Receiver<ShutdownState> {
        self.tx.subscribe()
    }
}

/// Includes shutdown requested before subscription or an earlier wait.
pub async fn wait_for_shutdown(rx: &mut watch::Receiver<ShutdownState>) -> ShutdownState {
    loop {
        let state = rx.borrow_and_update().clone();
        if state.is_shutdown() {
            return state;
        }
        if rx.changed().await.is_err() {
            return ShutdownState::Graceful;
        }
    }
}

pub async fn wait_for_immediate(rx: &mut watch::Receiver<ShutdownState>) -> ShutdownState {
    loop {
        let state = rx.borrow_and_update().clone();
        if state.is_immediate() {
            return state;
        }
        if rx.changed().await.is_err() {
            return ShutdownState::Immediate {
                error: rx.borrow().error().cloned(),
            };
        }
    }
}

#[derive(Clone, Debug, Snafu, PartialEq, Eq)]
pub enum ShutdownError {
    // For future work: It would be nice if we could keep the actual errors in here, but
    // `crate::Error` doesn't implement `Clone`, and adding `DynClone` for errors is tricky.
    #[snafu(display("The API failed to start: {error}"))]
    ApiFailed { error: String },
    #[snafu(display("Reload failed, and then failed to restore the previous config"))]
    ReloadFailedToRestore,
    #[snafu(display(r#"The task for source "{key}" died during execution: {error}"#))]
    SourceAborted { key: ComponentKey, error: String },
    #[snafu(display(r#"The task for transform "{key}" died during execution: {error}"#))]
    TransformAborted { key: ComponentKey, error: String },
    #[snafu(display(r#"The task for sink "{key}" died during execution: {error}"#))]
    SinkAborted { key: ComponentKey, error: String },
}

#[derive(Debug)]
pub enum ControlSignal {
    Reload(Box<ReloadSignal>),
    Shutdown(ShutdownSignal),
}

impl From<ReloadSignal> for ControlSignal {
    fn from(signal: ReloadSignal) -> Self {
        Self::Reload(Box::new(signal))
    }
}

impl From<ShutdownSignal> for ControlSignal {
    fn from(signal: ShutdownSignal) -> Self {
        Self::Shutdown(signal)
    }
}

pub struct Signals {
    pub handler: SignalHandler,
    pub reloads: ReloadReceiver,
    pub shutdown: watch::Receiver<ShutdownState>,
}

impl Default for Signals {
    fn default() -> Self {
        let (handler, reloads, shutdown) = SignalHandler::new();
        Self {
            handler,
            reloads,
            shutdown,
        }
    }
}

impl Signals {
    /// Creates channels and registers the OS signal handlers on the runtime.
    pub fn new(runtime: &Runtime) -> Self {
        let signals = Self::default();
        #[cfg(unix)]
        let stream = os_signals(runtime);
        #[cfg(windows)]
        let stream = os_signals();
        signals.handler.forever(runtime, stream);
        signals
    }
}

/// Routes OS and provider requests to the reload mailbox or shutdown state.
#[derive(Clone)]
pub struct SignalHandler {
    pub reloads: ReloadSender,
    pub shutdown: ShutdownSender,
    shutdown_txs: Vec<broadcast::Sender<()>>,
}

impl SignalHandler {
    pub fn new() -> (Self, ReloadReceiver, watch::Receiver<ShutdownState>) {
        let mailbox = Arc::new(ReloadMailbox::default());
        let reloads = ReloadSender {
            mailbox: Arc::downgrade(&mailbox),
        };
        let receiver = ReloadReceiver { mailbox };
        let (tx, shutdown_rx) = watch::channel(ShutdownState::Running);
        let handler = Self {
            reloads,
            shutdown: ShutdownSender { tx },
            shutdown_txs: Vec::new(),
        };
        (handler, receiver, shutdown_rx)
    }

    fn forward(&self, signal: ControlSignal) -> bool {
        match signal {
            ControlSignal::Reload(reload) => self.reloads.send(*reload).is_ok(),
            ControlSignal::Shutdown(shutdown) => {
                self.shutdown.send(shutdown);
                true
            }
        }
    }

    fn forever<T, S>(&self, runtime: &Runtime, stream: S)
    where
        T: Into<ControlSignal> + Send,
        S: Stream<Item = T> + 'static + Send,
    {
        let handler = self.clone();
        runtime.spawn(async move {
            tokio::pin!(stream);
            while let Some(value) = stream.next().await {
                if !handler.forward(value.into()) {
                    break;
                }
            }
        });
    }

    /// Forwards a provider stream until it closes or [`Self::clear`] cancels it.
    pub fn add<T, S>(&mut self, stream: S)
    where
        T: Into<ControlSignal> + Send,
        S: Stream<Item = T> + 'static + Send,
    {
        let (shutdown_tx, mut shutdown_rx) = broadcast::channel::<()>(2);
        // Do not capture cancellation senders in the forwarding task itself.
        let handler = Self {
            reloads: self.reloads.clone(),
            shutdown: self.shutdown.clone(),
            shutdown_txs: Vec::new(),
        };
        self.shutdown_txs.push(shutdown_tx);
        tokio::spawn(async move {
            tokio::pin!(stream);
            loop {
                tokio::select! {
                    biased;
                    _ = shutdown_rx.recv() => break,
                    value = stream.next() => {
                        let Some(value) = value else { break };
                        if !handler.forward(value.into()) {
                            break;
                        }
                    }
                }
            }
        });
    }

    /// Cancels the active provider signal streams.
    pub fn clear(&mut self) {
        for shutdown_tx in self.shutdown_txs.drain(..) {
            _ = shutdown_tx.send(());
        }
    }
}

#[cfg(unix)]
fn os_signals(runtime: &Runtime) -> impl Stream<Item = ControlSignal> + use<> {
    use tokio::signal::unix::{SignalKind, signal};

    runtime.block_on(async {
        let mut sigint = signal(SignalKind::interrupt()).expect("Failed to set up SIGINT handler.");
        let mut sigterm =
            signal(SignalKind::terminate()).expect("Failed to set up SIGTERM handler.");
        let mut sigquit = signal(SignalKind::quit()).expect("Failed to set up SIGQUIT handler.");
        let mut sighup = signal(SignalKind::hangup()).expect("Failed to set up SIGHUP handler.");

        async_stream::stream! {
            loop {
                let signal = tokio::select! {
                    _ = sigint.recv() => {
                        info!(message = "Signal received.", signal = "SIGINT");
                        ShutdownSignal::Graceful.into()
                    },
                    _ = sigterm.recv() => {
                        info!(message = "Signal received.", signal = "SIGTERM");
                        ShutdownSignal::Graceful.into()
                    },
                    _ = sigquit.recv() => {
                        info!(message = "Signal received.", signal = "SIGQUIT");
                        ShutdownSignal::Immediate.into()
                    },
                    _ = sighup.recv() => {
                        info!(message = "Signal received.", signal = "SIGHUP");
                        ReloadSignal::Disk.into()
                    },
                };
                yield signal;
            }
        }
    })
}

#[cfg(windows)]
fn os_signals() -> impl Stream<Item = ControlSignal> {
    async_stream::stream! {
        loop {
            _ = tokio::signal::ctrl_c().await;
            yield ShutdownSignal::Graceful.into();
        }
    }
}

#[cfg(test)]
mod tests {
    use futures::FutureExt;
    use tokio::time::{advance, timeout};

    use super::*;

    #[tokio::test]
    async fn shutdown_survives_reload_flood() {
        let (handler, _reloads, mut shutdown) = SignalHandler::new();
        for _ in 0..1000 {
            handler.reloads.send(ReloadSignal::Disk).unwrap();
        }
        handler.shutdown.send(ShutdownSignal::Graceful);
        assert_eq!(
            timeout(Duration::from_secs(1), wait_for_shutdown(&mut shutdown))
                .await
                .unwrap(),
            ShutdownState::Graceful,
        );
    }

    #[tokio::test]
    async fn shutdown_is_durable_for_late_and_existing_subscribers() {
        let (handler, _reloads, mut shutdown) = SignalHandler::new();
        handler.shutdown.send(ShutdownSignal::Graceful);
        let mut late = handler.shutdown.subscribe();
        assert_eq!(
            wait_for_shutdown(&mut late).now_or_never(),
            Some(ShutdownState::Graceful)
        );
        assert_eq!(
            wait_for_shutdown(&mut shutdown).now_or_never(),
            Some(ShutdownState::Graceful)
        );
        assert_eq!(
            wait_for_shutdown(&mut shutdown).now_or_never(),
            Some(ShutdownState::Graceful)
        );
        assert!(wait_for_immediate(&mut shutdown).now_or_never().is_none());

        handler.shutdown.send(ShutdownSignal::Graceful);
        for _ in 0..1000 {
            handler.shutdown.send(ShutdownSignal::Graceful);
        }
        assert_eq!(
            wait_for_immediate(&mut late).now_or_never(),
            Some(ShutdownState::Immediate { error: None })
        );
        assert_eq!(
            wait_for_shutdown(&mut shutdown).now_or_never(),
            Some(ShutdownState::Immediate { error: None })
        );
    }

    #[test]
    fn failed_shutdown_keeps_first_error_through_escalation() {
        let (handler, _reloads, shutdown) = SignalHandler::new();
        let first = ShutdownError::ReloadFailedToRestore;
        handler.shutdown.send(ShutdownSignal::Failed(first.clone()));
        assert_eq!(*shutdown.borrow(), ShutdownState::Failed(first.clone()));
        handler.shutdown.send(ShutdownSignal::Immediate);
        handler
            .shutdown
            .send(ShutdownSignal::Failed(ShutdownError::ApiFailed {
                error: "later".into(),
            }));
        assert_eq!(
            *shutdown.borrow(),
            ShutdownState::Immediate { error: Some(first) }
        );
    }

    #[test]
    fn immediate_shutdown_can_record_a_later_failure_without_regressing() {
        let (handler, _reloads, shutdown) = SignalHandler::new();
        handler.shutdown.send(ShutdownSignal::Immediate);
        handler
            .shutdown
            .send(ShutdownSignal::Failed(ShutdownError::ReloadFailedToRestore));
        assert_eq!(
            *shutdown.borrow(),
            ShutdownState::Immediate {
                error: Some(ShutdownError::ReloadFailedToRestore)
            }
        );
    }

    #[tokio::test]
    async fn closing_shutdown_channels_does_not_hang_waiters() {
        let (handler, _reloads, mut shutdown) = SignalHandler::new();
        drop(handler);
        assert_eq!(
            wait_for_shutdown(&mut shutdown).now_or_never(),
            Some(ShutdownState::Graceful)
        );
        assert_eq!(
            wait_for_immediate(&mut shutdown).now_or_never(),
            Some(ShutdownState::Immediate { error: None })
        );
    }

    #[tokio::test(start_paused = true)]
    async fn mixed_reloads_merge_without_losing_independent_work() {
        let (handler, mut reloads, _shutdown) = SignalHandler::new();
        let mut builder = ConfigBuilder::default();
        builder.global.data_dir = Some("/latest-builder".into());
        handler
            .reloads
            .send(ReloadSignal::Components(HashSet::from([
                ComponentKey::from("first"),
            ])))
            .unwrap();
        handler.reloads.send(ReloadSignal::Disk).unwrap();
        handler
            .reloads
            .send(ReloadSignal::EnrichmentTables)
            .unwrap();
        handler
            .reloads
            .send(ReloadSignal::ConfigBuilder(builder))
            .unwrap();
        handler
            .reloads
            .send(ReloadSignal::Components(HashSet::from([
                ComponentKey::from("first"),
                ComponentKey::from("second"),
            ])))
            .unwrap();

        let plan = reloads.recv().await;
        assert_eq!(
            plan.components,
            HashSet::from([ComponentKey::from("first"), ComponentKey::from("second")])
        );
        assert!(plan.enrichment_tables);
        assert!(plan.reload_external_files);
        let Some(ReloadConfig::Builder(builder)) = plan.config else {
            panic!("latest builder was lost")
        };
        assert_eq!(
            builder.global.data_dir.as_deref(),
            Some(std::path::Path::new("/latest-builder"))
        );
        assert!(reloads.recv().now_or_never().is_none());
    }

    #[tokio::test(start_paused = true)]
    async fn quiet_period_resets_and_cancelled_receive_keeps_the_plan() {
        let (handler, mut reloads, _shutdown) = SignalHandler::new();
        handler
            .reloads
            .send(ReloadSignal::ConfigBuilder(ConfigBuilder::default()))
            .unwrap();
        assert!(reloads.recv().now_or_never().is_none());
        advance(RELOAD_QUIET_PERIOD / 2).await;
        handler.reloads.send(ReloadSignal::Disk).unwrap();
        advance(RELOAD_QUIET_PERIOD / 2).await;
        assert!(reloads.recv().now_or_never().is_none());
        advance(RELOAD_QUIET_PERIOD / 2).await;
        let plan = reloads.recv().now_or_never().expect("quiet period elapsed");
        assert!(matches!(plan.config, Some(ReloadConfig::Disk)));
        assert!(plan.reload_external_files);
    }

    #[tokio::test(start_paused = true)]
    async fn reloads_during_application_remain_in_the_next_plan() {
        let (handler, mut reloads, _shutdown) = SignalHandler::new();
        handler.reloads.send(ReloadSignal::Disk).unwrap();
        let active = reloads.recv().await;
        handler
            .reloads
            .send(ReloadSignal::EnrichmentTables)
            .unwrap();
        assert!(matches!(active.config, Some(ReloadConfig::Disk)));
        assert!(!active.enrichment_tables);
        let pending = reloads.recv().await;
        assert!(pending.enrichment_tables);
        assert!(pending.config.is_none());
        assert!(!pending.reload_external_files);
    }

    #[test]
    fn reload_sender_reports_a_dropped_receiver() {
        let (handler, reloads, _shutdown) = SignalHandler::new();
        drop(reloads);
        assert!(handler.reloads.send(ReloadSignal::Disk).is_err());
    }
}
