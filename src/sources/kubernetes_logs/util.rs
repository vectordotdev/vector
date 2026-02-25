use std::{error::Error, future::Future, time::Duration};

use futures::{
    FutureExt, Sink,
    future::{Either, select},
    pin_mut,
};
use vector_lib::{
    file_source::{
        file_server::{FileServer, Line, Shutdown as FileServerShutdown},
        paths_provider::PathsProvider,
    },
    file_source_common::{Checkpointer, FileSourceInternalEvents},
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
    PP: PathsProvider + Send + Sync + 'static,
    E: FileSourceInternalEvents,
    C: Sink<Vec<Line>> + Unpin + Send + 'static,
    <C as Sink<Vec<Line>>>::Error: Error + Send,
    S: Future + Unpin + Send + 'static,
    <S as Future>::Output: Clone + Send + Sync,
    <<PP as PathsProvider>::IntoIter as IntoIterator>::IntoIter: Send,
{
    let span = info_span!("file_server");

    // spawn_blocking shouldn't be needed: https://github.com/vectordotdev/vector/issues/23743
    let join_handle = tokio::task::spawn_blocking(move || {
        // These will need to be separated when this source is updated
        // to support end-to-end acknowledgements.
        let shutdown = shutdown.shared();
        let shutdown2 = shutdown.clone();
        let _enter = span.enter();

        let rt = tokio::runtime::Handle::current();
        let result = rt.block_on(file_server.run(chans, shutdown, shutdown2, checkpointer));
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
