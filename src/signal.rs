use tokio::sync::{broadcast, mpsc};
use tokio_stream::{Stream, StreamExt};

use super::config::ConfigBuilder;

pub type ShutdownTx = broadcast::Sender<()>;
pub type SignalTx = mpsc::Sender<SignalTo>;
pub type SignalRx = mpsc::Receiver<SignalTo>;

#[derive(Debug)]
/// Control messages used by Vector to drive topology and shutdown events.
#[allow(clippy::large_enum_variant)] // discovered during Rust upgrade to 1.57; just allowing for now since we did previously
pub enum SignalTo {
    /// Signal to reload config from a string.
    ReloadFromConfigBuilder(ConfigBuilder),
    /// Signal to reload config from the filesystem.
    ReloadFromDisk,
    /// Signal to shutdown process.
    Shutdown,
    /// Shutdown process immediately.
    Quit,
}

/// SignalHandler is a general `ControlTo` message receiver and transmitter. It's used by
/// OS signals and providers to surface control events to the root of the application.
pub struct SignalHandler {
    tx: SignalTx,
    shutdown_txs: Vec<ShutdownTx>,
}

impl SignalHandler {
    /// Create a new signal handler. We'll have space for 2 control messages at a time, to
    /// ensure the channel isn't blocking.
    pub fn new() -> (Self, SignalRx) {
        let (tx, rx) = mpsc::channel(2);
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

    /// Takes a stream who's elements are convertible to `SignalTo`, and spawns a permanent
    /// task for transmitting to the receiver.
    pub fn forever<T, S>(&mut self, stream: S)
    where
        T: Into<SignalTo> + Send + Sync,
        S: Stream<Item = T> + 'static + Send,
    {
        let tx = self.tx.clone();

        tokio::spawn(async move {
            tokio::pin!(stream);

            while let Some(value) = stream.next().await {
                if tx.send(value.into()).await.is_err() {
                    error!(message = "Couldn't send signal.");
                    break;
                }
            }
        });
    }

    /// Takes a stream, sending to the underlying signal receiver. Returns a broadcast tx
    /// channel which can be used by the caller to either subscribe to cancelation, or trigger
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
                        if tx.send(value.into()).await.is_err() {
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
            let _ = shutdown_tx.send(());
        }
    }
}

/// Signals from OS/user.
#[cfg(unix)]
pub fn os_signals() -> impl Stream<Item = SignalTo> {
    use tokio::signal::unix::{signal, SignalKind};

    let mut sigint = signal(SignalKind::interrupt()).expect("Signal handlers should not panic.");
    let mut sigterm = signal(SignalKind::terminate()).expect("Signal handlers should not panic.");
    let mut sigquit = signal(SignalKind::quit()).expect("Signal handlers should not panic.");
    let mut sighup = signal(SignalKind::hangup()).expect("Signal handlers should not panic.");

    async_stream::stream! {
        loop {
            let signal = tokio::select! {
                _ = sigint.recv() => SignalTo::Shutdown,
                _ = sigterm.recv() => SignalTo::Shutdown,
                _ = sigquit.recv() => SignalTo::Quit,
                _ = sighup.recv() => SignalTo::ReloadFromDisk,
            };
            yield signal;
        }
    }
}

/// Signals from OS/user.
#[cfg(windows)]
pub fn os_signals() -> impl Stream<Item = SignalTo> {
    use futures::future::FutureExt;

    async_stream::stream! {
        loop {
            let signal = tokio::signal::ctrl_c().map(|_| SignalTo::Shutdown).await;
            yield signal;
        }
    }
}
