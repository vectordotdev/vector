use crate::config::Config;
use tokio::sync::{mpsc, oneshot};
use tokio_stream::{Stream, StreamExt};

pub type ShutdownTx = oneshot::Sender<()>;

#[derive(Debug)]
/// Control messages used by Vector to drive topology and shutdown events.
pub enum Control {
    /// Receive a new configuration.
    Config(Config),
    /// Signal to reload config from the filesystem.
    Reload,
    /// Signal to shutdown process.
    Shutdown,
    /// Shutdown process immediately.
    Quit,
}

/// Controller is a general `Control` message receiver and transmitter. It's typically used by
/// OS signals and providers to surface control events to the root of the application.
pub struct Controller {
    tx: mpsc::Sender<Control>,
    rx: Option<mpsc::Receiver<Control>>,
}

impl Controller {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel(2);

        Self { tx, rx: Some(rx) }
    }

    /// Takes a stream who's elements are convertible to `Control`, and spawns a permanent
    /// task for transmitting to the receiver.
    pub fn handler<T, S>(&mut self, stream: S)
    where
        T: Into<Control> + Send + Sync,
        S: Stream<Item = T> + 'static + Send + Sync,
    {
        let tx = self.tx.clone();

        tokio::spawn(async move {
            tokio::pin!(stream);

            while let Some(value) = stream.next().await {
                if tx.send(value.into()).await.is_err() {
                    error!(message = "Couldn't send control message");
                    break;
                }
            }
        });
    }

    /// Takes a stream who's elements are convertible to `Control`, and spawns a task that can
    /// be shut down by triggering the returned transmitter (typically, by falling out of scope.)
    pub fn with_shutdown<T, S>(&mut self, stream: S) -> ShutdownTx
    where
        T: Into<Control> + Send + Sync,
        S: Stream<Item = T> + 'static + Send + Sync,
    {
        let tx = self.tx.clone();

        let (shutdown_tx, mut shutdown_rx) = oneshot::channel();

        tokio::spawn(async move {
            tokio::pin!(stream);

            loop {
                tokio::select! {
                    biased;

                    _ = &mut shutdown_rx => break,
                    Some(value) = stream.next() => {
                        if tx.send(value.into()).await.is_err() {
                            error!("Couldn't send control message");
                            break;
                        }
                    },
                    else => unreachable!("controller doesn't end"),
                }
            }
        });

        shutdown_tx
    }

    /// Takes the receiver, replacing it with `None`. A controller is intended to have only one
    /// consumer, at the root of the application.
    pub fn take_rx(&mut self) -> Option<mpsc::Receiver<Control>> {
        self.rx.take()
    }
}
