use std::{
    collections::HashSet,
    fs, io,
    path::{Path, PathBuf},
    thread,
};

use futures::executor;
use tokio::sync::watch;
use vector_lib::{buffers::BufferConfig, config::ComponentKey};

use crate::{config::Config, internal_events::TopologyDiskBufferUnreferenced};

#[derive(Clone)]
struct ScanRequest {
    data_dir: PathBuf,
    configured_buffer_paths: HashSet<PathBuf>,
}

struct DiscoveredDiskBuffer {
    path: PathBuf,
    canonical_path: PathBuf,
}

// Scans are diagnostic only, so a watch channel retains just the latest reload request while the
// current blocking filesystem traversal finishes.
pub(super) struct OrphanedDiskBufferScanner {
    sender: watch::Sender<Option<ScanRequest>>,
}

impl OrphanedDiskBufferScanner {
    pub(super) fn new() -> Self {
        Self::spawn(discover_disk_buffers, report_orphaned_disk_buffers)
    }

    fn spawn<S, R>(scan: S, report: R) -> Self
    where
        S: Fn(&Path) -> io::Result<Vec<DiscoveredDiskBuffer>> + Send + 'static,
        R: Fn(&ScanRequest, io::Result<Vec<DiscoveredDiskBuffer>>) + Send + 'static,
    {
        let (sender, receiver) = watch::channel(None);
        if let Err(error) = thread::Builder::new()
            .name("orphaned-disk-buffer-scanner".into())
            .spawn(move || run_scanner(receiver, scan, report))
        {
            warn!(%error, message = "Failed to start unreferenced disk buffer scanner thread.");
        }
        Self { sender }
    }

    pub(super) fn scan(&self, config: &Config, temporary_exclusions: HashSet<PathBuf>) {
        let Some(data_dir) = config.global.data_dir.clone() else {
            return;
        };
        let configured_buffer_paths = referenced_disk_buffer_directories(config)
            .into_iter()
            .chain(temporary_exclusions)
            .collect();
        self.schedule(data_dir, configured_buffer_paths);
    }

    fn schedule(&self, data_dir: PathBuf, configured_buffer_paths: HashSet<PathBuf>) {
        self.sender.send_modify(move |pending| {
            *pending = Some(ScanRequest {
                data_dir,
                configured_buffer_paths,
            });
        });
    }
}

fn run_scanner<S, R>(mut receiver: watch::Receiver<Option<ScanRequest>>, scan: S, report: R)
where
    S: Fn(&Path) -> io::Result<Vec<DiscoveredDiskBuffer>>,
    R: Fn(&ScanRequest, io::Result<Vec<DiscoveredDiskBuffer>>),
{
    while executor::block_on(receiver.changed()).is_ok() {
        let Some(request) = receiver.borrow_and_update().clone() else {
            continue;
        };
        let result = scan(&request.data_dir);
        match receiver.has_changed() {
            Err(_) => return,
            Ok(false) => report(&request, result),
            Ok(true) => {}
        }
    }
}

fn report_orphaned_disk_buffers(
    request: &ScanRequest,
    result: io::Result<Vec<DiscoveredDiskBuffer>>,
) {
    let discovered_buffers = match result {
        Ok(discovered_buffers) => discovered_buffers,
        Err(error) => {
            warn!(
                data_dir = request.data_dir.to_string_lossy().as_ref(),
                %error,
                message = "Failed to scan for unreferenced disk buffers.",
            );
            return;
        }
    };

    let orphaned_buffers =
        filter_orphaned_disk_buffers(discovered_buffers, &request.configured_buffer_paths);
    let disk_buffer_root = request.data_dir.join("buffer").join("v2");
    for orphaned_buffer in orphaned_buffers {
        let Ok(orphaned_buffer_id) = orphaned_buffer.strip_prefix(&disk_buffer_root) else {
            warn!(
                buffer_dir = orphaned_buffer.to_string_lossy().as_ref(),
                buffer_root = disk_buffer_root.to_string_lossy().as_ref(),
                message = "Found unreferenced disk buffer outside the expected buffer root.",
            );
            continue;
        };
        emit!(TopologyDiskBufferUnreferenced {
            buffer_id: orphaned_buffer_id,
            buffer_dir: &orphaned_buffer,
        });
    }
}

pub(super) fn referenced_disk_buffer_directories(config: &Config) -> HashSet<PathBuf> {
    let data_dir = config.global.data_dir.as_deref();
    let sinks = config
        .sinks()
        .map(|(id, sink)| (id.clone(), sink.buffer.clone()));
    let enrichment_table_sinks = config
        .enrichment_tables()
        .filter_map(|(id, table)| table.as_sink(id))
        .map(|(id, sink)| (id, sink.buffer));

    disk_buffer_directories(data_dir, sinks.chain(enrichment_table_sinks))
}

fn disk_buffer_directories(
    data_dir: Option<&Path>,
    buffers: impl Iterator<Item = (ComponentKey, BufferConfig)>,
) -> HashSet<PathBuf> {
    buffers
        .flat_map(|(id, buffer)| {
            buffer
                .stages()
                .iter()
                .filter_map(|stage| stage.disk_usage(data_dir.map(Path::to_path_buf), &id))
                .map(|usage| usage.data_dir().to_path_buf())
                .collect::<Vec<_>>()
        })
        .collect()
}

#[cfg(test)]
fn find_orphaned_disk_buffers(
    data_dir: &Path,
    configured_buffer_paths: &HashSet<PathBuf>,
) -> io::Result<Vec<PathBuf>> {
    discover_disk_buffers(data_dir)
        .map(|buffers| filter_orphaned_disk_buffers(buffers, configured_buffer_paths))
}

fn discover_disk_buffers(data_dir: &Path) -> io::Result<Vec<DiscoveredDiskBuffer>> {
    let disk_buffer_root = data_dir.join("buffer").join("v2");
    let canonical_root = match fs::canonicalize(&disk_buffer_root) {
        Ok(root) => root,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };

    let mut pending = vec![disk_buffer_root.clone()];
    let mut visited = HashSet::new();
    let mut discovered_buffers = Vec::new();
    while let Some(path) = pending.pop() {
        let Some(canonical_path) = canonical_path_within_root(&path, &canonical_root) else {
            continue;
        };
        if !visited.insert(canonical_path.clone()) {
            continue;
        }

        let entries = match ignore_not_found(fs::read_dir(&path)) {
            Ok(Some(entries)) => entries,
            Ok(None) => continue,
            Err(error) => {
                warn!(
                    path = path.to_string_lossy().as_ref(),
                    %error,
                    message = "Failed to read disk buffer directory.",
                );
                continue;
            }
        };
        let mut child_directories = Vec::new();
        let mut has_disk_buffer_file = false;
        for entry in entries {
            let entry = match entry {
                Ok(entry) => entry,
                Err(error) => {
                    warn!(
                        path = path.to_string_lossy().as_ref(),
                        %error,
                        message = "Failed to read disk buffer directory entry.",
                    );
                    continue;
                }
            };
            let entry_path = entry.path();
            let Some(canonical_entry_path) =
                canonical_path_within_root(&entry_path, &canonical_root)
            else {
                continue;
            };
            let metadata = match ignore_not_found(fs::metadata(&entry_path)) {
                Ok(Some(metadata)) => metadata,
                Ok(None) => continue,
                Err(error) => {
                    warn!(
                        path = entry_path.to_string_lossy().as_ref(),
                        %error,
                        message = "Failed to read disk buffer entry metadata.",
                    );
                    continue;
                }
            };
            if metadata.is_dir() {
                if !visited.contains(&canonical_entry_path) {
                    child_directories.push(entry_path);
                }
            } else if metadata.is_file() && is_disk_buffer_file(&entry_path) {
                // A valid data file is also a marker so damaged buffers missing their ledger are
                // still diagnosed. The data filename validation below intentionally mirrors the
                // private canonical disk_v2 parser without widening the vector-buffers API.
                has_disk_buffer_file = true;
            }
        }
        if path != disk_buffer_root && has_disk_buffer_file {
            discovered_buffers.push(DiscoveredDiskBuffer {
                path,
                canonical_path,
            });
        }
        child_directories.sort_by(|left, right| right.cmp(left));
        pending.extend(child_directories);
    }
    discovered_buffers.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(discovered_buffers)
}

fn filter_orphaned_disk_buffers(
    discovered_buffers: Vec<DiscoveredDiskBuffer>,
    configured_buffer_paths: &HashSet<PathBuf>,
) -> Vec<PathBuf> {
    let configured_canonical_paths = configured_buffer_paths
        .iter()
        .filter_map(|path| fs::canonicalize(path).ok())
        .collect::<HashSet<_>>();

    discovered_buffers
        .into_iter()
        .filter(|buffer| {
            !configured_buffer_paths.contains(&buffer.path)
                && !configured_canonical_paths.contains(&buffer.canonical_path)
        })
        .map(|buffer| buffer.path)
        .collect()
}

fn is_disk_buffer_file(path: &Path) -> bool {
    if path.file_name().is_some_and(|name| name == "buffer.db") {
        return true;
    }
    path.file_name()
        .and_then(|name| name.to_str())
        .and_then(|name| name.strip_prefix("buffer-data-"))
        .and_then(|name| name.strip_suffix(".dat"))
        .and_then(|id| id.parse::<u16>().ok())
        .is_some_and(|id| id < u16::MAX)
}

fn canonical_path_within_root(path: &Path, canonical_root: &Path) -> Option<PathBuf> {
    match ignore_not_found(fs::canonicalize(path)) {
        Ok(Some(path)) if path.starts_with(canonical_root) => Some(path),
        Ok(Some(target)) => {
            warn!(
                path = path.to_string_lossy().as_ref(),
                target = target.to_string_lossy().as_ref(),
                message = "Skipping disk buffer path outside the buffer root.",
            );
            None
        }
        Ok(None) => None,
        Err(error) => {
            warn!(
                path = path.to_string_lossy().as_ref(),
                %error,
                message = "Failed to resolve disk buffer path.",
            );
            None
        }
    }
}

fn ignore_not_found<T>(result: io::Result<T>) -> io::Result<Option<T>> {
    match result {
        Ok(value) => Ok(Some(value)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::HashSet, fs, sync::mpsc, time::Duration};

    #[cfg(unix)]
    use std::os::unix::fs::symlink;

    use tempfile::tempdir;

    use super::{OrphanedDiskBufferScanner, find_orphaned_disk_buffers};

    fn recv<T>(receiver: &mpsc::Receiver<T>) -> T {
        receiver.recv_timeout(Duration::from_secs(2)).unwrap()
    }

    #[test]
    fn runtime_shutdown_does_not_wait_for_scan() {
        let (release_tx, release_rx) = mpsc::channel();
        let (started_tx, started_rx) = mpsc::channel();
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .unwrap();

        runtime.block_on(async {
            let scanner = OrphanedDiskBufferScanner::spawn(
                move |_| {
                    started_tx.send(()).unwrap();
                    release_rx.recv().unwrap();
                    Ok(Vec::new())
                },
                |_, _| {},
            );
            scanner.schedule("scan".into(), HashSet::new());
            recv(&started_rx);
            drop(scanner);
        });

        let (runtime_dropped_tx, runtime_dropped_rx) = mpsc::channel();
        std::thread::spawn(move || {
            drop(runtime);
            runtime_dropped_tx.send(()).unwrap();
        });
        runtime_dropped_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("runtime shutdown waited for the detached scan");
        release_tx.send(()).unwrap();
    }

    #[test]
    fn finds_only_unconfigured_disk_buffer_directories() {
        let data_dir = tempdir().unwrap();
        let buffer_root = data_dir.path().join("buffer").join("v2");
        let namespace = buffer_root.join("namespace");
        let configured = namespace.join("configured");
        let nested_orphaned = configured.join("nested-orphaned");
        let orphaned = namespace.join("orphaned");
        let damaged = namespace.join("damaged");
        let overlapping_orphan = namespace.join("overlapping-orphan");
        let configured_descendant = overlapping_orphan.join("configured-descendant");
        let unrelated = buffer_root.join("unrelated");
        fs::create_dir_all(&configured).unwrap();
        fs::create_dir(&nested_orphaned).unwrap();
        fs::create_dir(&orphaned).unwrap();
        fs::create_dir(&damaged).unwrap();
        fs::create_dir_all(&configured_descendant).unwrap();
        fs::create_dir(&unrelated).unwrap();
        fs::write(configured.join("buffer.db"), b"configured").unwrap();
        fs::write(nested_orphaned.join("buffer.db"), b"nested orphan").unwrap();
        fs::write(orphaned.join("buffer.db"), b"ledger").unwrap();
        fs::write(damaged.join("buffer-data-42.dat"), b"data").unwrap();
        fs::write(overlapping_orphan.join("buffer.db"), b"overlapping orphan").unwrap();
        fs::write(configured_descendant.join("buffer.db"), b"configured").unwrap();
        fs::write(unrelated.join("other.db"), b"unrelated").unwrap();
        fs::write(unrelated.join("buffer-data-65535.dat"), b"reserved").unwrap();
        fs::write(buffer_root.join("not-a-buffer"), b"ignored").unwrap();

        let configured_paths = HashSet::from([configured, configured_descendant]);
        let actual = find_orphaned_disk_buffers(data_dir.path(), &configured_paths).unwrap();

        assert_eq!(
            actual,
            vec![nested_orphaned, damaged, orphaned, overlapping_orphan]
        );
    }

    #[cfg(unix)]
    #[test]
    fn safely_follows_symlinked_buffer_directories_and_files() {
        let data_dir = tempdir().unwrap();
        let external_dir = tempdir().unwrap();
        let buffer_root = data_dir.path().join("buffer").join("v2");
        let namespace = buffer_root.join("namespace");
        let linked_buffer = namespace.join("a-linked-buffer");
        let buffer_target = namespace.join("z-buffer-target");
        let linked_file_buffer = namespace.join("linked-file-buffer");
        let file_target = namespace.join("ledger-target");
        fs::create_dir_all(&buffer_target).unwrap();
        fs::create_dir(&linked_file_buffer).unwrap();
        fs::write(buffer_target.join("buffer.db"), b"linked directory").unwrap();
        fs::write(&file_target, b"linked file").unwrap();
        fs::write(external_dir.path().join("buffer.db"), b"outside root").unwrap();
        symlink(&buffer_target, &linked_buffer).unwrap();
        symlink(&file_target, linked_file_buffer.join("buffer.db")).unwrap();
        symlink(&namespace, namespace.join("cycle")).unwrap();
        symlink(external_dir.path(), namespace.join("outside-root")).unwrap();

        let actual = find_orphaned_disk_buffers(data_dir.path(), &HashSet::new()).unwrap();
        assert_eq!(actual, vec![linked_buffer, linked_file_buffer]);
    }
}
