use crossterm::event::{poll, read, Event, KeyCode};
use tokio::sync::{mpsc, oneshot};

static INPUT_INVARIANT: &str = "Couldn't capture keyboard input. Please report.";

/// Capture keyboard input, and send it upstream via a channel. This is used for interaction
/// with the dashboard, and exiting from `vector top`.
pub fn capture_key_press() -> (mpsc::Receiver<KeyCode>, oneshot::Sender<()>) {
    let (tx, rx) = mpsc::channel(5);
    let (kill_tx, mut kill_rx) = oneshot::channel();

    tokio::spawn(async move {
        loop {
            if poll(std::time::Duration::from_millis(250)).unwrap_or(false) {
                if let Event::Key(k) = read().expect(INPUT_INVARIANT) {
                    let _ = tx.clone().send(k.code).await;
                };
            } else if kill_rx.try_recv().is_ok() {
                return;
            }
        }
    });

    (rx, kill_tx)
}
