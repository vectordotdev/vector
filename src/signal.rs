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

use super::config::{ComponentKey, ConfigBuilder};

pub type ShutdownTx = broadcast::Sender<()>;
pub type SignalTx = broadcast::Sender<SignalTo>;
pub type SignalRx = broadcast::Receiver<SignalTo>;

#[derive(Debug, Clone)]
/// Control messages used by Vector to drive topology and shutdown events.
#[allow(clippy::large_enum_variant)] // discovered during Rust upgrade to 1.57; just allowing for now since we did previously
pub enum SignalTo {
    /// Signal to reload given components.
    ReloadComponents(HashSet<ComponentKey>),
    /// Signal to reload config from a string.
    ReloadFromConfigBuilder(ConfigBuilder),
    /// Signal to reload config from the filesystem and reload components with external files.
    ReloadFromDisk,
    /// Signal to reload all enrichment tables.
    ReloadEnrichmentTables,
    /// Signal to shutdown process.
    Shutdown(Option<ShutdownError>),
    /// Shutdown process immediately.
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
            (Shutdown(a), Shutdown(b)) => a == b,
            (Quit, Quit) => true,
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

/// Convenience struct for app setup handling.
pub struct SignalPair {
    pub handler: SignalHandler,
    pub receiver: SignalRx,
}

impl SignalPair {
    /// Create a new signal handler pair, and set them up to receive OS signals.
    pub fn new(runtime: &Runtime) -> Self {
        let (handler, receiver) = SignalHandler::new();

        #[cfg(unix)]
        let signals = os_signals(runtime);

        // If we passed `runtime` here, we would get the following:
        // error[E0521]: borrowed data escapes outside of associated function
        #[cfg(windows)]
        let signals = os_signals();

        handler.forever(runtime, signals);
        Self { handler, receiver }
    }
}

/// SignalHandler is a general `ControlTo` message receiver and transmitter. It's used by
/// OS signals and providers to surface control events to the root of the application.
pub struct SignalHandler {
    tx: SignalTx,
    shutdown_txs: Vec<ShutdownTx>,
}

impl SignalHandler {
    /// Create a new signal handler with space for 128 control messages at a time, to
    /// ensure the channel doesn't overflow and drop signals.
    pub fn new() -> (Self, SignalRx) {
        let (tx, rx) = broadcast::channel(128);
        let handler = Self {
            tx,
            shutdown_txs: vec![],
        };

        (handler, rx)
    }

    /// Clones the transmitter.
    pub fn clone_tx(&self) -> SignalTx {
        self.tx.clone()
    }

    /// Subscribe to the stream, and return a new receiver.
    pub fn subscribe(&self) -> SignalRx {
        self.tx.subscribe()
    }

    /// Takes a stream who's elements are convertible to `SignalTo`, and spawns a permanent
    /// task for transmitting to the receiver.
    fn forever<T, S>(&self, runtime: &Runtime, stream: S)
    where
        T: Into<SignalTo> + Send + Sync,
        S: Stream<Item = T> + 'static + Send,
    {
        let tx = self.tx.clone();

        runtime.spawn(async move {
            tokio::pin!(stream);

            while let Some(value) = stream.next().await {
                if tx.send(value.into()).is_err() {
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
        T: Into<SignalTo> + Send,
        S: Stream<Item = T> + 'static + Send,
    {
        let (shutdown_tx, mut shutdown_rx) = broadcast::channel::<()>(2);
        let tx = self.tx.clone();

        self.shutdown_txs.push(shutdown_tx);

        tokio::spawn(async move {
            tokio::pin!(stream);

            loop {
                tokio::select! {
                    biased;

                    _ = shutdown_rx.recv() => break,
                    Some(value) = stream.next() => {
                        if tx.send(value.into()).is_err() {
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

/// Routes a received signal: shutdown signals are returned, reload signals are forwarded to the
/// reload sink. Shared by the shutdown receive helpers below.
fn classify_signal(signal: SignalTo, on_reload: &mut impl FnMut(SignalTo)) -> Option<SignalTo> {
    match signal {
        SignalTo::Shutdown(_) | SignalTo::Quit => Some(signal),
        reload @ (SignalTo::ReloadFromDisk
        | SignalTo::ReloadComponents(_)
        | SignalTo::ReloadFromConfigBuilder(_)
        | SignalTo::ReloadEnrichmentTables) => {
            on_reload(reload);
            None
        }
    }
}

/// Resolves when a shutdown signal (or a closed signal channel) is received from the signal
/// receiver. Reload signals received along the way are forwarded to `on_reload` (e.g. so startup
/// can re-broadcast them once it completes); lagged receivers are consumed and ignored.
pub async fn recv_shutdown(rx: &mut SignalRx, mut on_reload: impl FnMut(SignalTo)) -> SignalTo {
    loop {
        match rx.recv().await {
            Ok(signal) => {
                if let Some(shutdown) = classify_signal(signal, &mut on_reload) {
                    return shutdown;
                }
            }
            Err(RecvError::Closed) => return SignalTo::Shutdown(None),
            Err(RecvError::Lagged(_)) => {}
        }
    }
}

/// Non-blocking counterpart of [`recv_shutdown`]: drains the signal receiver, returning the
/// shutdown signal if one is queued (consuming reload and lagged signals along the way), or
/// `None` once the queue is empty.
pub fn try_recv_shutdown(
    rx: &mut SignalRx,
    mut on_reload: impl FnMut(SignalTo),
) -> Option<SignalTo> {
    loop {
        match rx.try_recv() {
            Ok(signal) => {
                if let Some(shutdown) = classify_signal(signal, &mut on_reload) {
                    return Some(shutdown);
                }
            }
            Err(TryRecvError::Closed) => return Some(SignalTo::Shutdown(None)),
            Err(TryRecvError::Lagged(_)) => {}
            Err(TryRecvError::Empty) => return None,
        }
    }
}

/// Signals from OS/user.
#[cfg(unix)]
fn os_signals(runtime: &Runtime) -> impl Stream<Item = SignalTo> + use<> {
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
                        SignalTo::Shutdown(None)
                    },
                    _ = sigterm.recv() => {
                        info!(message = "Signal received.", signal = "SIGTERM");
                        SignalTo::Shutdown(None)
                    } ,
                    _ = sigquit.recv() => {
                        info!(message = "Signal received.", signal = "SIGQUIT");
                        SignalTo::Quit
                    },
                    _ = sighup.recv() => {
                        info!(message = "Signal received.", signal = "SIGHUP");
                        SignalTo::ReloadFromDisk
                    },
                };
                yield signal;
            }
        }
    })
}

/// Signals from OS/user.
#[cfg(windows)]
fn os_signals() -> impl Stream<Item = SignalTo> {
    use futures::future::FutureExt;

    async_stream::stream! {
        loop {
            let signal = tokio::signal::ctrl_c().map(|_| SignalTo::Shutdown(None)).await;
            yield signal;
        }
    }
}
