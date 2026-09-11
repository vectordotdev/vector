#![allow(missing_docs)]

use std::collections::HashSet;

use snafu::Snafu;
use tokio::{
    runtime::Runtime,
    sync::broadcast::{
        self,
        error::{RecvError, TryRecvError},
    },
};
use tokio_stream::{Stream, StreamExt};
use tracing::warn;

use super::config::{ComponentKey, ConfigBuilder};

pub type ShutdownTx = broadcast::Sender<()>;
pub type SignalTx = broadcast::Sender<SignalTo>;
pub type SignalRx = broadcast::Receiver<SignalTo>;
pub type ShutdownSignalTx = broadcast::Sender<ShutdownSignal>;
pub type ShutdownSignalRx = broadcast::Receiver<ShutdownSignal>;

/// Capacity of both the reload and shutdown channels. Sized so that neither overflows in
/// normal operation; the lag policies in this module handle the rest.
const CHANNEL_CAPACITY: usize = 128;

#[derive(Debug, Clone)]
/// Control messages used by Vector to drive topology reload events.
#[allow(clippy::large_enum_variant)]
pub enum SignalTo {
    /// Signal to reload given components.
    ReloadComponents(HashSet<ComponentKey>),
    /// Signal to reload config from a string.
    ReloadFromConfigBuilder(ConfigBuilder),
    /// Signal to reload config from the filesystem and reload components with external files.
    ReloadFromDisk,
    /// Signal to reload all enrichment tables.
    ReloadEnrichmentTables,
}

/// Shutdown messages, carried on a dedicated channel so that a flood of reload signals
/// cannot overflow the receiver and drop a shutdown.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ShutdownSignal {
    /// Gracefully drain and shut down the process.
    Graceful(Option<ShutdownError>),
    /// Shut down the process immediately.
    Quit,
}

impl PartialEq for SignalTo {
    fn eq(&self, other: &Self) -> bool {
        use SignalTo::*;

        match (self, other) {
            (ReloadComponents(a), ReloadComponents(b)) => a == b,
            // TODO: This will require a lot of plumbing but ultimately we can derive equality for config builders.
            (ReloadFromConfigBuilder(_), ReloadFromConfigBuilder(_)) => true,
            (ReloadFromDisk, ReloadFromDisk) => true,
            (ReloadEnrichmentTables, ReloadEnrichmentTables) => true,
            _ => false,
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

/// A signal received by the [`SignalHandler`], already routed to the reload or shutdown
/// channel.
#[derive(Debug)]
pub enum SignalOrShutdown {
    Reload(Box<SignalTo>),
    Shutdown(ShutdownSignal),
}

impl From<SignalTo> for SignalOrShutdown {
    fn from(signal: SignalTo) -> Self {
        Self::Reload(Box::new(signal))
    }
}

impl From<ShutdownSignal> for SignalOrShutdown {
    fn from(signal: ShutdownSignal) -> Self {
        Self::Shutdown(signal)
    }
}

/// Convenience struct for app setup handling.
pub struct SignalPair {
    pub handler: SignalHandler,
    pub receiver: SignalRx,
    pub shutdown_receiver: ShutdownReceiver,
}

impl SignalPair {
    /// Create a new signal handler pair, and set them up to receive OS signals.
    pub fn new(runtime: &Runtime) -> Self {
        let (handler, receiver, shutdown_receiver) = SignalHandler::new();
        let shutdown_receiver = ShutdownReceiver::new(shutdown_receiver);

        #[cfg(unix)]
        let signals = os_signals(runtime);

        // If we passed `runtime` here, we would get the following:
        // error[E0521]: borrowed data escapes outside of associated function
        #[cfg(windows)]
        let signals = os_signals();

        handler.forever(runtime, signals);
        Self {
            handler,
            receiver,
            shutdown_receiver,
        }
    }
}

/// SignalHandler is a general `ControlTo` message receiver and transmitter. It's used by
/// OS signals and providers to surface control events to the root of the application.
#[derive(Clone)]
pub struct SignalHandler {
    tx: SignalTx,
    shutdown_tx: ShutdownSignalTx,
    shutdown_txs: Vec<ShutdownTx>,
}

impl SignalHandler {
    /// Create a new signal handler with space for 128 control messages at a time, to
    /// ensure the channel doesn't overflow and drop signals.
    pub fn new() -> (Self, SignalRx, ShutdownSignalRx) {
        let (tx, rx) = broadcast::channel(CHANNEL_CAPACITY);
        // Shutdown signals live on their own channel so a burst of reloads overflowing
        // the signal channel can never drop a shutdown or quit.
        let (shutdown_tx, shutdown_rx) = broadcast::channel(CHANNEL_CAPACITY);
        let handler = Self {
            tx,
            shutdown_tx,
            shutdown_txs: vec![],
        };

        (handler, rx, shutdown_rx)
    }

    /// Clones the reload transmitter.
    pub fn clone_tx(&self) -> SignalTx {
        self.tx.clone()
    }

    /// Clones the shutdown transmitter.
    pub fn clone_shutdown_tx(&self) -> ShutdownSignalTx {
        self.shutdown_tx.clone()
    }

    /// Subscribe to the reload stream, and return a new receiver.
    pub fn subscribe(&self) -> SignalRx {
        self.tx.subscribe()
    }

    /// Subscribe to the shutdown channel, and return a new receiver.
    pub fn subscribe_shutdown(&self) -> ShutdownReceiver {
        ShutdownReceiver::new(self.shutdown_tx.subscribe())
    }

    /// Sends a shutdown signal to the root of the application.
    pub fn send_shutdown(&self, signal: ShutdownSignal) {
        if self.shutdown_tx.send(signal).is_err() {
            error!(
                message = "Couldn't send shutdown signal.",
                internal_log_rate_limit = false
            );
        }
    }

    /// Takes a stream whose elements are convertible to [`SignalOrShutdown`], and spawns a
    /// permanent task for transmitting to the receivers.
    fn forever<T, S>(&self, runtime: &Runtime, stream: S)
    where
        T: Into<SignalOrShutdown> + Send + Sync,
        S: Stream<Item = T> + 'static + Send,
    {
        let tx = self.tx.clone();
        let shutdown_tx = self.shutdown_tx.clone();

        runtime.spawn(async move {
            tokio::pin!(stream);

            while let Some(value) = stream.next().await {
                let failed = match value.into() {
                    SignalOrShutdown::Reload(reload) => tx.send(*reload).is_err(),
                    SignalOrShutdown::Shutdown(shutdown) => shutdown_tx.send(shutdown).is_err(),
                };
                if failed {
                    error!(
                        message = "Couldn't send signal.",
                        internal_log_rate_limit = false
                    );
                    break;
                }
            }
        });
    }

    /// Takes a stream, sending to the underlying signal receiver. Returns a broadcast tx
    /// channel which can be used by the caller to either subscribe to cancellation, or trigger
    /// it. Useful for providers that may need to do both.
    pub fn add<T, S>(&mut self, stream: S)
    where
        T: Into<SignalOrShutdown> + Send,
        S: Stream<Item = T> + 'static + Send,
    {
        let (shutdown_tx, mut shutdown_rx) = broadcast::channel::<()>(2);
        let tx = self.tx.clone();
        let shutdown_signal_tx = self.shutdown_tx.clone();

        self.shutdown_txs.push(shutdown_tx);

        tokio::spawn(async move {
            tokio::pin!(stream);

            loop {
                tokio::select! {
                    biased;

                    _ = shutdown_rx.recv() => break,
                    Some(value) = stream.next() => {
                        let failed = match value.into() {
                            SignalOrShutdown::Reload(reload) => tx.send(*reload).is_err(),
                            SignalOrShutdown::Shutdown(shutdown) => {
                                shutdown_signal_tx.send(shutdown).is_err()
                            }
                        };
                        if failed {
                            error!(message = "Couldn't send signal.", internal_log_rate_limit = false);
                            break;
                        }
                    }
                    else => {
                        error!(message = "Underlying stream is closed.", internal_log_rate_limit = false);
                        break;
                    }
                }
            }
        });
    }

    /// Shutdown active signal handlers.
    pub fn clear(&mut self) {
        for shutdown_tx in self.shutdown_txs.drain(..) {
            // An error just means the channel was already shut down; safe to ignore.
            _ = shutdown_tx.send(());
        }
    }
}

/// Wrapper around the raw shutdown receiver that enforces the shutdown contract.
///
/// A second shutdown signal means force-quit. The tokio broadcast receiver reports lag
/// when more shutdowns arrive than the channel holds while the receiver is busy, so any
/// lag means multiple shutdowns were sent and dropped — at least a second one. By the
/// contract, that resolves to an immediate quit rather than a graceful shutdown.
pub struct ShutdownReceiver {
    rx: ShutdownSignalRx,
}

impl ShutdownReceiver {
    pub const fn new(rx: ShutdownSignalRx) -> Self {
        Self { rx }
    }

    /// Receives the next shutdown signal, resolving when one arrives. A closed channel
    /// resolves to a graceful shutdown; lag resolves to a quit per the contract above.
    pub async fn recv(&mut self) -> ShutdownSignal {
        match self.rx.recv().await {
            Ok(shutdown) => shutdown,
            Err(RecvError::Closed) => ShutdownSignal::Graceful(None),
            Err(RecvError::Lagged(amt)) => Self::on_lag(amt),
        }
    }

    /// Non-blocking counterpart of [`ShutdownReceiver::recv`], returning `None` when no
    /// shutdown is queued.
    pub fn try_recv(&mut self) -> Option<ShutdownSignal> {
        match self.rx.try_recv() {
            Ok(shutdown) => Some(shutdown),
            Err(TryRecvError::Closed) => Some(ShutdownSignal::Graceful(None)),
            Err(TryRecvError::Lagged(amt)) => Some(Self::on_lag(amt)),
            Err(TryRecvError::Empty) => None,
        }
    }

    fn on_lag(amt: u64) -> ShutdownSignal {
        error!(
            message = "Overflow, dropped {} shutdown signals; quitting immediately.",
            amt
        );
        ShutdownSignal::Quit
    }
}

/// Resolves when a shutdown signal (or a closed shutdown channel) is received. Reload
/// signals received along the way are forwarded to `on_reload` (e.g. so startup can
/// re-broadcast them once it completes). Lag on the reload channel is logged; lag on the
/// shutdown channel means multiple shutdowns were sent, which quits immediately.
pub async fn recv_shutdown(
    rx: &mut SignalRx,
    shutdown_rx: &mut ShutdownReceiver,
    mut on_reload: impl FnMut(SignalTo),
) -> ShutdownSignal {
    // Check the reload channel first so reloads are never re-ordered ahead of an
    // already-queued shutdown; a pending shutdown still wins the next poll.
    loop {
        match rx.try_recv() {
            Ok(reload) => on_reload(reload),
            Err(TryRecvError::Lagged(amt)) => {
                warn!(message = "Overflow, dropped {} signals.", amt)
            }
            Err(TryRecvError::Closed | TryRecvError::Empty) => break,
        }
    }
    shutdown_rx.recv().await
}

/// Non-blocking counterpart of [`recv_shutdown`]: drains the signal receiver, returning the
/// shutdown signal if one is queued (consuming reload signals along the way), or `None`
/// once the queue is empty. Shutdown-channel lag means multiple shutdowns were sent,
/// which quits immediately.
pub fn try_recv_shutdown(
    rx: &mut SignalRx,
    shutdown_rx: &mut ShutdownReceiver,
    mut on_reload: impl FnMut(SignalTo),
) -> Option<ShutdownSignal> {
    loop {
        match rx.try_recv() {
            Ok(reload) => on_reload(reload),
            Err(TryRecvError::Lagged(amt)) => {
                warn!(message = "Overflow, dropped {} signals.", amt)
            }
            Err(TryRecvError::Closed | TryRecvError::Empty) => break,
        }
    }
    shutdown_rx.try_recv()
}

/// Signals from OS/user.
#[cfg(unix)]
fn os_signals(runtime: &Runtime) -> impl Stream<Item = SignalOrShutdown> + use<> {
    use tokio::signal::unix::{SignalKind, signal};

    // The `signal` function must be run within the context of a Tokio runtime.
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
                        SignalOrShutdown::Shutdown(ShutdownSignal::Graceful(None))
                    },
                    _ = sigterm.recv() => {
                        info!(message = "Signal received.", signal = "SIGTERM");
                        SignalOrShutdown::Shutdown(ShutdownSignal::Graceful(None))
                    } ,
                    _ = sigquit.recv() => {
                        info!(message = "Signal received.", signal = "SIGQUIT");
                        SignalOrShutdown::Shutdown(ShutdownSignal::Quit)
                    },
                    _ = sighup.recv() => {
                        info!(message = "Signal received.", signal = "SIGHUP");
                        SignalOrShutdown::Reload(Box::new(SignalTo::ReloadFromDisk))
                    },
                };
                yield signal;
            }
        }
    })
}

/// Signals from OS/user.
#[cfg(windows)]
fn os_signals() -> impl Stream<Item = SignalOrShutdown> {
    use futures::future::FutureExt;

    async_stream::stream! {
        loop {
            let signal = tokio::signal::ctrl_c()
                .map(|_| SignalOrShutdown::Shutdown(ShutdownSignal::Graceful(None)))
                .await;
            yield signal;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::{Duration, timeout};

    #[tokio::test]
    async fn shutdown_survives_reload_flood() {
        // A burst of reloads big enough to overflow the reload channel must not
        // delay or drop a concurrently-sent shutdown.
        let (handler, mut rx, shutdown_rx) = SignalHandler::new();
        let mut shutdown_rx = ShutdownReceiver::new(shutdown_rx);
        for _ in 0..1000 {
            drop(handler.clone_tx().send(SignalTo::ReloadFromDisk));
        }
        handler.send_shutdown(ShutdownSignal::Graceful(None));

        let received = timeout(Duration::from_secs(1), async {
            recv_shutdown(&mut rx, &mut shutdown_rx, |_| {}).await
        })
        .await
        .expect("shutdown not received within timeout");

        assert_eq!(received, ShutdownSignal::Graceful(None));
    }

    #[tokio::test]
    async fn try_recv_shutdown_finds_queued_shutdown() {
        let (handler, mut rx, shutdown_rx) = SignalHandler::new();
        let mut shutdown_rx = ShutdownReceiver::new(shutdown_rx);
        assert!(try_recv_shutdown(&mut rx, &mut shutdown_rx, |_| {}).is_none());
        handler.send_shutdown(ShutdownSignal::Quit);
        assert_eq!(
            try_recv_shutdown(&mut rx, &mut shutdown_rx, |_| {}),
            Some(ShutdownSignal::Quit)
        );
    }

    #[test]
    fn shutdown_lag_quits_immediately() {
        let (handler, _rx, shutdown_rx) = SignalHandler::new();
        let mut shutdown_rx = ShutdownReceiver::new(shutdown_rx);

        // Exceed the shutdown channel capacity while the receiver is idle. Lag means
        // multiple shutdowns were sent and dropped — at least a second one — which by
        // the shutdown contract is an immediate quit, not a graceful shutdown.
        for _ in 0..(CHANNEL_CAPACITY + 1) {
            handler.send_shutdown(ShutdownSignal::Graceful(None));
        }
        assert_eq!(shutdown_rx.try_recv(), Some(ShutdownSignal::Quit));

        // The queued (non-dropped) shutdowns remain graceful when received individually.
        assert_eq!(shutdown_rx.try_recv(), Some(ShutdownSignal::Graceful(None)));
        assert_eq!(shutdown_rx.try_recv(), Some(ShutdownSignal::Graceful(None)));
    }
}
