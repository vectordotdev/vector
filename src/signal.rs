use tokio::sync::{broadcast, mpsc};
use tokio_stream::{Stream, StreamExt};

pub type ShutdownTx = broadcast::Sender<()>;
pub type SignalTx = mpsc::Sender<SignalTo>;
pub type SignalRx = mpsc::Receiver<SignalTo>;

#[derive(Debug)]
/// Control messages used by Vector to drive topology and shutdown events.
pub enum SignalTo {
    /// Signal to reload config from a string.
    ReloadFromString(String),
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
    rx: Option<SignalRx>,
    shutdown_txs: Vec<ShutdownTx>,
}

impl SignalHandler {
    /// Create a new signal handler. We'll have space for 2 control messages at a time, to
    /// ensure the channel isn't blocking.
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel(2);

        Self {
            tx,
            rx: Some(rx),
            shutdown_txs: vec![],
        }
    }

    /// Clones the transmitter.
    pub fn clone_tx(&self) -> SignalTx {
        self.tx.clone()
    }

    /// Takes a stream who's elements are convertible to `SignalTo`, and spawns a permanent
    /// task for transmitting to the receiver.
    pub fn add<T, S>(&mut self, stream: S)
    where
        T: Into<SignalTo> + Send + Sync,
        S: Stream<Item = T> + 'static + Send + Sync,
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
    pub fn with_shutdown<T, S>(&mut self, stream: S) -> ShutdownTx
    where
        T: Into<SignalTo> + Send + Sync,
        S: Stream<Item = T> + 'static + Send + Sync,
    {
        let (shutdown_tx, mut shutdown_rx) = broadcast::channel::<()>(2);
        let tx = self.tx.clone();

        self.shutdown_txs.push(shutdown_tx.clone());

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

        shutdown_tx
    }

    /// Triggers a shutdown on the underlying broadcast channel(s).
    pub fn trigger_shutdown(&mut self) {
        for shutdown_tx in self.shutdown_txs.drain(..) {
            // An error just means the channel was already shut down; safe to ignore.
            let _ = shutdown_tx.send(());
        }
    }

    /// Takes the receiver, replacing it with `None`. A controller is intended to have only one
    /// consumer, typically at the root of the application.
    pub fn take_rx(&mut self) -> Option<SignalRx> {
        self.rx.take()
    }
}

impl Default for SignalHandler {
    fn default() -> Self {
        Self::new()
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
