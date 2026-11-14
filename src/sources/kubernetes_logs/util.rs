use std::{error::Error, future::Future, time::Duration};

use futures::{
    Sink,
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
/// The checkpoint shutdown must wait for the line receiver to finish processing.
pub(super) async fn run_file_server<PP, E, C, S1, S2>(
    file_server: FileServer<PP, E>,
    chans: C,
    shutdown: S1,
    shutdown_checkpointer: S2,
    checkpointer: Checkpointer,
) -> Result<FileServerShutdown, tokio::task::JoinError>
where
    PP: PathsProvider + Send + Sync + 'static,
    E: FileSourceInternalEvents,
    C: Sink<Vec<Line>> + Unpin + Send + 'static,
    <C as Sink<Vec<Line>>>::Error: Error + Send,
    S1: Future + Unpin + Send + 'static,
    S2: Future + Unpin + Send + 'static,
    <<PP as PathsProvider>::IntoIter as IntoIterator>::IntoIter: Send,
{
    let span = info_span!("file_server");

    // spawn_blocking shouldn't be needed: https://github.com/vectordotdev/vector/issues/23743
    let join_handle = tokio::task::spawn_blocking(move || {
        let _enter = span.enter();

        let rt = tokio::runtime::Handle::current();
        let result =
            rt.block_on(file_server.run(chans, shutdown, shutdown_checkpointer, checkpointer));
        result.expect("file server exited with an error")
    });
    join_handle.await
}

pub(super) async fn complete_with_deadline_on_signal<F, S>(
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

#[cfg(test)]
mod tests {
    use bytes::Bytes;
    use futures::{StreamExt, channel::mpsc};
    use vector_lib::{
        file_source::paths_provider::{Glob, MatchOptions},
        file_source_common::{FingerprintStrategy, Fingerprinter, ReadFrom},
    };

    use super::*;
    use crate::internal_events::FileSourceInternalEventsEmitter;

    #[tokio::test]
    async fn final_checkpoints_wait_for_receiver_completion() {
        assert_final_checkpoint_after_receiver(false).await;
    }

    #[tokio::test]
    async fn checkpoint_shutdown_handles_receiver_cancellation() {
        assert_final_checkpoint_after_receiver(true).await;
    }

    async fn assert_final_checkpoint_after_receiver(cancel_completion: bool) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("app.log");
        std::fs::write(&path, "tick\n").unwrap();
        let emitter = FileSourceInternalEventsEmitter {
            include_file_metric_tag: false,
        };
        let file_server = FileServer {
            paths_provider: Glob::new(
                std::slice::from_ref(&path),
                &[],
                MatchOptions::default(),
                emitter.clone(),
            )
            .unwrap(),
            max_read_bytes: 2048,
            ignore_checkpoints: false,
            read_from: ReadFrom::Beginning,
            ignore_before: None,
            max_line_bytes: 1024,
            line_delimiter: Bytes::from_static(b"\n"),
            data_dir: dir.path().to_owned(),
            glob_minimum_cooldown: Duration::from_mins(1),
            fingerprinter: Fingerprinter::new(
                FingerprintStrategy::FirstLinesChecksum {
                    ignored_header_bytes: 0,
                    lines: 1,
                },
                1024,
                false,
            ),
            oldest_first: false,
            remove_after: None,
            emitter,
            rotate_wait: Duration::from_mins(1),
        };
        let checkpointer = Checkpointer::new(dir.path());
        let checkpoints = checkpointer.view();
        let (tx, rx) = mpsc::channel(2);
        let mut lines = rx.flat_map(futures::stream::iter);
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
        let (receiver_done_tx, receiver_done_rx) = tokio::sync::oneshot::channel::<()>();
        let mut server = tokio::spawn(run_file_server(
            file_server,
            tx,
            shutdown_rx,
            receiver_done_rx,
            checkpointer,
        ));

        let first = lines.next().await.unwrap();
        assert_eq!(first.text, "tick");
        checkpoints.update(first.file_id, first.end_offset);
        std::fs::write(&path, "tick\nFINAL\n").unwrap();
        shutdown_tx.send(()).unwrap();

        let final_line = lines.next().await.unwrap();
        assert_eq!(final_line.text, "FINAL");
        assert!(lines.next().await.is_none());
        // Closing the producer must not finish checkpointing while the receiver
        // has yet to process the final batch and update its offsets.
        assert!(
            tokio::time::timeout(Duration::from_millis(100), &mut server)
                .await
                .is_err(),
            "file server finished before the receiver updated final checkpoints"
        );
        checkpoints.update(final_line.file_id, final_line.end_offset);
        if cancel_completion {
            drop(receiver_done_tx);
        } else {
            receiver_done_tx.send(()).unwrap();
        }
        server.await.unwrap().unwrap();

        let mut persisted = Checkpointer::new(dir.path());
        persisted.read_checkpoints(None).await;
        assert_eq!(
            persisted.view().get(final_line.file_id),
            Some(final_line.end_offset)
        );
    }
}
