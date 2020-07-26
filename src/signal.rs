use futures::{compat::Stream01CompatExt, Stream, StreamExt, TryStreamExt};
use futures01::{Future as Future01, Stream as Stream01};

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
    use tokio_signal::unix::{Signal, SIGHUP, SIGINT, SIGQUIT, SIGTERM};

    let sigint = Signal::new(SIGINT).flatten_stream();
    let sigterm = Signal::new(SIGTERM).flatten_stream();
    let sigquit = Signal::new(SIGQUIT).flatten_stream();
    let sighup = Signal::new(SIGHUP).flatten_stream();

    let signals = sigint.select(sigterm.select(sigquit.select(sighup)));

    signals
        .map(|signal| match signal {
            SIGHUP => SignalTo::Reload,
            SIGINT | SIGTERM => SignalTo::Shutdown,
            SIGQUIT => SignalTo::Quit,
            _ => unreachable!(),
        })
        .compat()
        .into_stream()
        .map(|result| result.expect("Neither stream errors"))
}

/// Signals from OS/user.
#[cfg(windows)]
pub fn signals() -> impl Stream<Item = SignalTo> {
    let ctrl_c = tokio_signal::ctrl_c().flatten_stream();

    ctrl_c
        .map(|_| SignalTo::Shutdown)
        .compat()
        .into_stream()
        .map(|result| result.expect("Shouldn't error"))
}
