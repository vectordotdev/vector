use futures::{
    Stream,
};

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum SignalTo {
    /// Signal to reload config.
    Reload,
    /// Signal to shutdown process.
    Shutdown,
    /// Shutdown process immediately.
    Quit,
}

/// Signals from OS/user.
#[cfg(unix)]
pub fn signals() -> impl Stream<Item = SignalTo> + 'static {
    use tokio::signal::unix::{signal, SignalKind};

    let mut sigint = signal(SignalKind::interrupt()).expect("Signal handlers should not panic.");
    let mut sigterm = signal(SignalKind::terminate()).expect("Signal handlers should not panic.");
    let mut sigquit = signal(SignalKind::quit()).expect("Signal handlers should not panic.");
    let mut sighup = signal(SignalKind::hangup()).expect("Signal handlers should not panic.");

    async_stream::stream! {
        let signal = tokio::select! {
            _ = sigint.recv() => SignalTo::Shutdown,
            _ = sigterm.recv() => SignalTo::Shutdown,
            _ = sigquit.recv() => SignalTo::Quit,
            _ = sighup.recv() => SignalTo::Reload,
        };
        yield signal;
    }
}

/// Signals from OS/user.
#[cfg(windows)]
pub fn signals() -> impl Stream<Item = SignalTo> {
    let ctrl_c = tokio::signal::ctrl_c();

    ctrl_c.map(|_| SignalTo::Shutdown).into_stream()
}
