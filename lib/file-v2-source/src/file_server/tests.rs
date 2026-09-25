use super::*;
use crate::FingerprintStrategy;
use bytes::BytesMut;
use futures::{FutureExt, StreamExt};
use std::{io::Error, num::NonZeroUsize, path::Path};
use tokio::sync::{mpsc, oneshot};

struct ScriptedPaths {
    updates: mpsc::UnboundedReceiver<PathUpdates>,
    requests: mpsc::UnboundedSender<()>,
}

impl PathsProvider for ScriptedPaths {
    async fn paths(&mut self, _: bool) -> PathUpdates {
        self.requests.send(()).unwrap();
        self.updates.recv().await.unwrap()
    }

    async fn wait_for_changes(&mut self) {}
}

#[derive(Clone)]
struct Events {
    small: mpsc::UnboundedSender<PathBuf>,
}

impl FileSourceInternalEvents for Events {
    fn emit_file_added(&self, _: &Path) {}
    fn emit_file_resumed(&self, _: &Path, _: u64) {}
    fn emit_file_watch_error(&self, _: &Path, error: Error) {
        panic!("{error}");
    }
    fn emit_file_unwatched(&self, _: &Path, _: bool) {}
    fn emit_file_deleted(&self, _: &Path) {}
    fn emit_file_delete_error(&self, _: &Path, _: Error) {}
    fn emit_file_fingerprint_read_error(&self, _: &Path, _: Error) {}
    fn emit_file_checkpointed(&self, _: usize, _: Duration) {}
    fn emit_file_checksum_failed(&self, path: &Path) {
        self.small.send(path.to_owned()).unwrap();
    }
    fn emit_file_checkpoint_write_error(&self, error: Error) {
        panic!("{error}");
    }
    fn emit_files_open(&self, _: usize) {}
    fn emit_path_globbing_failed(&self, _: &Path, _: &Error) {}
    fn emit_file_line_too_long(&self, _: &BytesMut, _: usize, _: usize) {}
}

// Control discovery boundaries while using actual readers, files and checkpoints.
struct Server {
    _data: tempfile::TempDir,
    updates: mpsc::UnboundedSender<PathUpdates>,
    requests: mpsc::UnboundedReceiver<()>,
    small: mpsc::UnboundedReceiver<PathBuf>,
    lines: futures::channel::mpsc::Receiver<Vec<Line>>,
    checkpoints: Arc<CheckpointsView>,
    stop: Option<oneshot::Sender<()>>,
    task: tokio::task::JoinHandle<Result<Shutdown, futures::channel::mpsc::SendError>>,
}

impl Server {
    fn start() -> Self {
        let data = tempfile::tempdir().unwrap();
        let checkpointer = Checkpointer::new(data.path());
        let checkpoints = checkpointer.view();
        let (updates, rx) = mpsc::unbounded_channel();
        let (requests, request_rx) = mpsc::unbounded_channel();
        let (small, small_rx) = mpsc::unbounded_channel();
        let server = FileServer {
            paths_provider: ScriptedPaths {
                updates: rx,
                requests,
            },
            max_read_bytes: 64,
            ignore_checkpoints: false,
            read_from: ReadFrom::Beginning,
            ignore_before: None,
            max_line_bytes: 128,
            line_delimiter: Bytes::from_static(b"\n"),
            fingerprinter: Fingerprinter::new(
                FingerprintStrategy::FirstBytesChecksum {
                    ignored_header_bytes: 0,
                    bytes: NonZeroUsize::new(8).unwrap(),
                },
                128,
                false,
            ),
            remove_after: None,
            emitter: Events { small },
            reader_idle_timeout: Duration::ZERO,
            checkpoint_interval: Duration::from_secs(1),
            test_sender: None,
        };
        let (send_lines, lines) = futures::channel::mpsc::channel(8);
        let (stop, shutdown) = oneshot::channel();
        let shutdown = shutdown.shared();
        let task = tokio::spawn(server.run(send_lines, shutdown.clone(), shutdown, checkpointer));
        Self {
            _data: data,
            updates,
            requests: request_rx,
            small: small_rx,
            lines,
            checkpoints,
            stop: Some(stop),
            task,
        }
    }

    async fn requested(&mut self) {
        timeout(Duration::from_secs(5), self.requests.recv())
            .await
            .unwrap()
            .unwrap();
    }

    fn snapshot(&self, paths: &[PathBuf]) {
        self.updates
            .send(PathUpdates::Snapshot(paths.iter().cloned().collect()))
            .unwrap();
    }

    async fn finish(mut self) {
        self.stop.take().unwrap().send(()).unwrap();
        self.snapshot(&[]);
        timeout(Duration::from_secs(5), &mut self.task)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert!(self.lines.next().await.is_none());
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.task.abort();
    }
}

#[tokio::test]
async fn snapshots_forget_vanished_small_files_without_remove_notifications() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("small.log");
    fs::write(&path, "small").await.unwrap();
    let mut server = Server::start();
    server.requested().await;
    server.snapshot(std::slice::from_ref(&path));
    assert_eq!(
        timeout(Duration::from_secs(5), server.small.recv())
            .await
            .unwrap(),
        Some(path.clone())
    );
    server.requested().await;
    fs::remove_file(&path).await.unwrap();
    server.snapshot(&[]);
    server.requested().await;
    fs::write(&path, "other").await.unwrap();
    server.snapshot(std::slice::from_ref(&path));
    server.requested().await;
    assert_eq!(
        server.small.try_recv().ok(),
        Some(path),
        "a new small file at a vanished path must not inherit the old suppression entry"
    );
    server.finish().await;
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn non_utf8_paths_emit_complete_and_retired_partial_records() {
    use std::{ffi::OsString, os::unix::ffi::OsStringExt};
    let root = tempfile::tempdir().unwrap();
    let path = root
        .path()
        .join(OsString::from_vec(b"invalid-\xff.log".to_vec()));
    fs::write(&path, "complete line\npartial").await.unwrap();
    let mut server = Server::start();
    server.requested().await;
    server.snapshot(std::slice::from_ref(&path));
    server.requested().await;
    server.snapshot(std::slice::from_ref(&path));
    let batch = timeout(Duration::from_secs(5), server.lines.next())
        .await
        .unwrap()
        .expect("source must not panic on a valid native path");
    assert_eq!(batch[0].text, "complete line");
    assert_eq!(batch[0].filename, path.to_string_lossy());
    server.requested().await;
    fs::remove_file(&path).await.unwrap();
    server.snapshot(&[]);
    let batch = timeout(Duration::from_secs(5), server.lines.next())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(batch[0].text, "partial");
    assert_eq!(batch[0].filename, path.to_string_lossy());
    server.requested().await;
    server.finish().await;
}

#[tokio::test]
async fn reopening_at_eof_keeps_the_checkpoint_past_retirement_expiry() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("input.log");
    let contents = b"already read\n";
    fs::write(&path, contents).await.unwrap();
    let (small, _small_rx) = mpsc::unbounded_channel();
    let fingerprint = Fingerprinter::new(
        FingerprintStrategy::FirstBytesChecksum {
            ignored_header_bytes: 0,
            bytes: NonZeroUsize::new(8).unwrap(),
        },
        128,
        false,
    )
    .fingerprint_or_emit(&path, &mut HashMap::new(), &Events { small })
    .await
    .unwrap();
    let mut server = Server::start();
    server.requested().await;
    server
        .checkpoints
        .update(fingerprint, contents.len() as u64);
    server.checkpoints.set_dead(fingerprint);
    server.snapshot(std::slice::from_ref(&path));
    server.requested().await;
    server.snapshot(std::slice::from_ref(&path));
    server.requested().await;
    // Expiration uses wall-clock UTC, not Tokio's clock. Wait past the real
    // 60-second grace to exercise revival without adding a test-only clock API.
    tokio::time::sleep(Duration::from_secs(61)).await;
    server.checkpoints.remove_expired();
    assert_eq!(
        server.checkpoints.get(fingerprint),
        Some(contents.len() as u64)
    );
    server.finish().await;
}

#[cfg(unix)]
#[tokio::test]
async fn non_utf8_path_metadata_preserves_complete_and_partial_record_contents() {
    use std::{ffi::OsString, os::unix::ffi::OsStringExt};
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("input.log");
    fs::write(&path, "complete line\npartial").await.unwrap();
    let mut watcher = FileWatcher::new(
        path,
        ReadFrom::Beginning,
        None,
        128,
        Bytes::from_static(b"\n"),
    )
    .await
    .unwrap();
    // APFS rejects non-UTF-8 directory entries. Exercise native path metadata
    // independently here; the Linux test above covers discovery and file I/O.
    watcher.path = root
        .path()
        .join(OsString::from_vec(b"invalid-\xff.log".to_vec()));
    let id = FileFingerprint::FirstBytesChecksum(1);
    let ReadResult::Line(raw) = watcher.read_line_bounded(128).await.unwrap() else {
        panic!("expected complete line");
    };
    let line = Line::from_watcher(raw, id, &watcher);
    assert_eq!(line.text, "complete line");
    assert_eq!(line.filename, watcher.path.to_string_lossy());
    assert!(matches!(
        watcher.read_line_bounded(128).await.unwrap(),
        ReadResult::Eof
    ));
    let ReadResult::Line(raw) = watcher.finish_partial() else {
        panic!("expected partial line");
    };
    let line = Line::from_watcher(raw, id, &watcher);
    assert_eq!(line.text, "partial");
    assert_eq!(line.filename, watcher.path.to_string_lossy());
}

// Exercise the discovery call site as well as DeliveryProgress itself: after
// the replacement has been read there are no more bytes to repair a lost offset.
#[tokio::test]
async fn fingerprint_migration_preserves_early_and_late_acknowledgements() {
    for acknowledge_before_discovery in [true, false] {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("input.log");
        fs::write(&path, "initial contents\n").await.unwrap();
        let mut server = Server::start();
        server.requested().await;
        server.snapshot(std::slice::from_ref(&path));
        server.requested().await;
        server.snapshot(std::slice::from_ref(&path));
        server.requested().await;
        let first = timeout(Duration::from_secs(5), server.lines.next())
            .await
            .unwrap()
            .unwrap()
            .remove(0);
        first
            .delivery_progress
            .checkpoint(&server.checkpoints, first.file_id, first.end_offset);
        fs::write(&path, "new content\n").await.unwrap();
        // Read the observed shrink before discovery supplies the new fingerprint.
        server
            .updates
            .send(PathUpdates::Changed {
                updated: Default::default(),
                removed: Default::default(),
            })
            .unwrap();
        server.requested().await;
        let replacement = timeout(Duration::from_secs(5), server.lines.next())
            .await
            .unwrap()
            .unwrap()
            .remove(0);
        assert_eq!(replacement.text, "new content");
        assert_eq!(replacement.file_id, first.file_id);
        let progress = &replacement.delivery_progress;
        if acknowledge_before_discovery {
            progress.checkpoint(
                &server.checkpoints,
                replacement.file_id,
                replacement.end_offset,
            );
        }
        let (small, _rx) = mpsc::unbounded_channel();
        let new_id = Fingerprinter::new(
            FingerprintStrategy::FirstBytesChecksum {
                ignored_header_bytes: 0,
                bytes: NonZeroUsize::new(8).unwrap(),
            },
            128,
            false,
        )
        .fingerprint_or_emit(&path, &mut HashMap::new(), &Events { small })
        .await
        .unwrap();
        assert_ne!(new_id, first.file_id);
        server.snapshot(std::slice::from_ref(&path));
        server.requested().await;
        if !acknowledge_before_discovery {
            progress.checkpoint(
                &server.checkpoints,
                replacement.file_id,
                replacement.end_offset,
            );
        }
        assert_eq!(
            server.checkpoints.get(new_id),
            Some(replacement.end_offset),
            "checkpoint must survive migration; early ack={acknowledge_before_discovery}"
        );
        assert_eq!(server.checkpoints.get(first.file_id), None);
        server.finish().await;
    }
}
