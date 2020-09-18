use futures::{
    future::{select_all, BoxFuture},
    FutureExt, Stream,
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
pub fn signals() -> impl Stream<Item = SignalTo> {
    use tokio::signal::unix::{signal, SignalKind};

    let mut sigint = signal(SignalKind::interrupt()).expect("Signal handlers should not panic.");
    let mut sigterm = signal(SignalKind::terminate()).expect("Signal handlers should not panic.");
    let mut sigquit = signal(SignalKind::quit()).expect("Signal handlers should not panic.");
    let mut sighup = signal(SignalKind::hangup()).expect("Signal handlers should not panic.");

    let set: Vec<BoxFuture<SignalTo>> = vec![
        Box::pin(async move { sigint.recv().map(|_| SignalTo::Shutdown).await }),
        Box::pin(async move { sigterm.recv().map(|_| SignalTo::Shutdown).await }),
        Box::pin(async move { sigquit.recv().map(|_| SignalTo::Quit).await }),
        Box::pin(async move { sighup.recv().map(|_| SignalTo::Reload).await }),
    ];

    let selection = select_all(set.into_iter()).map(|(val, _, _)| val);

    selection.into_stream()
}

/// Signals from OS/user.
#[cfg(windows)]
pub fn signals() -> impl Stream<Item = SignalTo> {
    let ctrl_c = tokio::signal::ctrl_c();

    ctrl_c.map(|_| SignalTo::Shutdown).into_stream()
}
