use std::{convert::Infallible, error::Error, future::Future, time::Duration};

use file_source::{
    paths_provider::PathsProvider, Checkpointer, FileServer, FileServerShutdown,
    FileSourceInternalEvents, Line,
};
use futures::{
    future::{select, Either},
    pin_mut, Sink,
};
use tokio::task::spawn_blocking;

/// A tiny wrapper around a [`FileServer`] that runs it as a [`spawn_blocking`]
/// task.
pub async fn run_file_server<PP, E, C, S>(
    file_server: FileServer<PP, E>,
    chans: C,
    shutdown: S,
    checkpointer: Checkpointer,
) -> Result<FileServerShutdown, tokio::task::JoinError>
where
    PP: PathsProvider + Send + 'static,
    E: FileSourceInternalEvents,
    C: Sink<Vec<Line>> + Unpin + Send + 'static,
    <C as Sink<Vec<Line>>>::Error: Error + Send,
    S: Future + Unpin + Send + 'static,
    <S as Future>::Output: Clone + Send + Sync,
{
    let span = info_span!("file_server");
    let join_handle = spawn_blocking(move || {
        let _enter = span.enter();
        let result = file_server.run(chans, shutdown, checkpointer);
        result.expect("file server exited with an error")
    });
    join_handle.await
}

/// Takes a `future` returning a result with an [`Infallible`] Ok-value and
/// a `signal`, and returns a future that completes when the `future` errors or
/// the `signal` completes.
/// If `signal` is sent or cancelled, the `future` is dropped (and not polled
/// anymore).
pub async fn cancel_on_signal<E, F, S>(future: F, signal: S) -> Result<(), E>
where
    F: Future<Output = Result<Infallible, E>>,
    S: Future<Output = ()>,
{
    pin_mut!(future);
    pin_mut!(signal);
    match select(future, signal).await {
        Either::Left((future_result, _)) => match future_result {
            Ok(_infallible) => unreachable!("ok value is infallible, thus impossible to reach"),
            Err(err) => Err(err),
        },
        Either::Right(((), _)) => Ok(()),
    }
}

pub async fn complete_with_deadline_on_signal<F, S>(
    future: F,
    signal: S,
    deadline: Duration,
) -> Result<<F as Future>::Output, tokio::time::error::Elapsed>
where
    F: Future,
    S: Future<Output = ()>,
{
    pin_mut!(future);
    pin_mut!(signal);
    let future = match select(future, signal).await {
        Either::Left((future_output, _)) => return Ok(future_output),
        Either::Right(((), future)) => future,
    };
    pin_mut!(future);
    tokio::time::timeout(deadline, future).await
}
