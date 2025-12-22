use crossterm::event::{Event, EventStream, KeyCode};
use futures::StreamExt;
use tokio::sync::{mpsc, oneshot};

/// Capture keyboard input, and send it upstream via a channel. This is used for interaction
/// with the dashboard, and exiting from `vector top`.
pub fn capture_key_press() -> (mpsc::UnboundedReceiver<KeyCode>, oneshot::Sender<()>) {
    let (tx, rx) = mpsc::unbounded_channel();
    let (kill_tx, mut kill_rx) = oneshot::channel();

    let mut events = EventStream::new();

    tokio::spawn(async move {
        loop {
            tokio::select! {
                biased;
                _ = &mut kill_rx => return,
                Some(Ok(event)) = events.next() => {
                     if let Event::Key(k) = event {
                        _ = tx.clone().send(k.code);
                    };
                }
            }
        }
    });

    (rx, kill_tx)
}
