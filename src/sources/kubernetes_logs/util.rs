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
        // These will need to be separated when this source is updated
        // to support end-to-end acknowledgements.
        let shutdown = shutdown.shared();
        let shutdown2 = shutdown.clone();
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
