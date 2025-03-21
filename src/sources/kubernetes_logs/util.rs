use std::{error::Error, future::Future, time::Duration};

use futures::{
    future::{select, Either},
    pin_mut, FutureExt, Sink,
};
use tokio::task::spawn_blocking;
use vector_lib::file_source::{
    paths_provider::PathsProvider, Checkpointer, FileServer, FileServerShutdown,
    FileSourceInternalEvents, Line,
};

/// A tiny wrapper around a [`FileServer`] that runs it as a [`spawn_blocking`]
/// task.
pub async fn run_file_server<PP, E, C, S, S2>(
    file_server: FileServer<PP, E>,
    chans: C,
    shutdown: S,
    shutdown2: S2,
    checkpointer: Checkpointer,
) -> Result<FileServerShutdown, tokio::task::JoinError>
where
    PP: PathsProvider + Send + 'static,
    E: FileSourceInternalEvents,
    C: Sink<Vec<Line>> + Unpin + Send + 'static,
    <C as Sink<Vec<Line>>>::Error: Error + Send,
    S: Future + Unpin + Send + 'static,
    <S as Future>::Output: Clone + Send + Sync,
    S2: Future + Unpin + Send + 'static,
    <S2 as Future>::Output: Clone + Send + Sync,
{
    let span = info_span!("file_server");
    let join_handle = spawn_blocking(move || {
        let shutdown = shutdown.shared();
        let shutdown2 = shutdown2.shared();
        let _enter = span.enter();
        let result = file_server.run(chans, shutdown, shutdown2, checkpointer);
        result.expect("file server exited with an error")
    });
    join_handle.await
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
