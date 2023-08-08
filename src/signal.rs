#![allow(missing_docs)]

use snafu::Snafu;
use tokio::{runtime::Runtime, sync::broadcast};
use tokio_stream::{Stream, StreamExt};

use super::config::{ComponentKey, ConfigBuilder};

pub type ShutdownTx = broadcast::Sender<()>;
pub type SignalTx = broadcast::Sender<SignalTo>;
pub type SignalRx = broadcast::Receiver<SignalTo>;

#[derive(Debug, Clone)]
/// Control messages used by Vector to drive topology and shutdown events.
#[allow(clippy::large_enum_variant)] // discovered during Rust upgrade to 1.57; just allowing for now since we did previously
pub enum SignalTo {
    /// Signal to reload config from a string.
    ReloadFromConfigBuilder(ConfigBuilder),
    /// Signal to reload config from the filesystem.
    ReloadFromDisk,
    /// Signal to shutdown process.
    Shutdown(Option<ShutdownError>),
    /// Shutdown process immediately.
    Quit,
}

#[derive(Clone, Debug, Snafu)]
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
        let signals = os_signals(runtime);
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
    fn new() -> (Self, SignalRx) {
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
                    error!(message = "Couldn't send signal.");
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
                            error!(message = "Couldn't send signal.");
                            break;
                        }
                    }
                    else => {
                        error!(message = "Underlying stream is closed.");
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

/// Signals from OS/user.
#[cfg(unix)]
fn os_signals(runtime: &Runtime) -> impl Stream<Item = SignalTo> {
    use tokio::signal::unix::{signal, SignalKind};

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
fn os_signals(_: &Runtime) -> impl Stream<Item = SignalTo> {
    use futures::future::FutureExt;

    async_stream::stream! {
        loop {
            let signal = tokio::signal::ctrl_c().map(|_| SignalTo::Shutdown(None)).await;
            yield signal;
        }
    }
}
