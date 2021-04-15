use crate::control::Control;
use futures::Stream;

/// Signals from OS/user.
#[cfg(unix)]
pub fn signals() -> impl Stream<Item = Control> {
    use tokio::signal::unix::{signal, SignalKind};

    let mut sigint = signal(SignalKind::interrupt()).expect("Signal handlers should not panic.");
    let mut sigterm = signal(SignalKind::terminate()).expect("Signal handlers should not panic.");
    let mut sigquit = signal(SignalKind::quit()).expect("Signal handlers should not panic.");
    let mut sighup = signal(SignalKind::hangup()).expect("Signal handlers should not panic.");

    async_stream::stream! {
        loop {
            let signal = tokio::select! {
                _ = sigint.recv() => Control::Shutdown,
                _ = sigterm.recv() => Control::Shutdown,
                _ = sigquit.recv() => Control::Quit,
                _ = sighup.recv() => Control::Reload,
            };
            yield signal;
        }
    }
}

/// Signals from OS/user.
#[cfg(windows)]
pub fn signals() -> impl Stream<Item = SignalTo> {
    use futures::future::FutureExt;

    async_stream::stream! {
        loop {
            let signal = tokio::signal::ctrl_c().map(|_| Control::Shutdown).await;
            yield signal;
        }
    }
}
