use bytes05::Bytes;
use file_source::{paths_provider::PathsProvider, FileServer, FileServerShutdown};
use futures::future::{select, Either};
use futures::{pin_mut, Sink};
use std::convert::Infallible;
use std::error::Error;
use std::{future::Future, time::Duration};
use tokio::task::spawn_blocking;

/// A tiny wrapper around a [`FileServer`] that runs it as a [`spawn_blocking`]
/// task.
pub async fn run_file_server<PP, C, S>(
    file_server: FileServer<PP>,
    chans: C,
    shutdown: S,
) -> Result<FileServerShutdown, tokio::task::JoinError>
where
    PP: PathsProvider + Send + 'static,
    C: Sink<(Bytes, String)> + Unpin + Send + 'static,
    <C as Sink<(Bytes, String)>>::Error: Error + Send,
    S: Future + Unpin + Send + 'static,
{
    let span = info_span!("file_server");
    let join_handle = spawn_blocking(move || {
        let _enter = span.enter();
        let result = file_server.run(chans, shutdown);
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
) -> Result<<F as Future>::Output, tokio::time::Elapsed>
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
