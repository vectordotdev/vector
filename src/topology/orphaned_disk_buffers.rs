use std::{
    collections::HashSet,
    fs, io,
    path::{Path, PathBuf},
    thread,
};

use futures::executor;
use tokio::sync::watch;
use vector_lib::{buffers::BufferConfig, config::ComponentKey};

use crate::config::Config;

#[derive(Clone)]
struct ScanRequest {
    generation: u64,
    data_dir: PathBuf,
    configured_buffer_paths: HashSet<PathBuf>,
}

// Scans are diagnostic only, so a watch channel retains just the latest reload request while the
// current blocking filesystem traversal finishes.
pub(super) struct OrphanedDiskBufferScanner {
    sender: watch::Sender<Option<ScanRequest>>,
}

impl OrphanedDiskBufferScanner {
    pub(super) fn new() -> Self {
        Self::spawn(scan_orphaned_disk_buffers, report_orphaned_disk_buffers)
    }

    fn spawn<S, R>(scan: S, report: R) -> Self
    where
        S: Fn(&ScanRequest) -> io::Result<Vec<PathBuf>> + Send + 'static,
        R: Fn(&ScanRequest, io::Result<Vec<PathBuf>>) + Send + 'static,
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
            let generation = pending
                .as_ref()
                .map_or(1, |request| request.generation.wrapping_add(1));
            *pending = Some(ScanRequest {
                generation,
                data_dir,
                configured_buffer_paths,
            });
        });
    }
}

fn run_scanner<S, R>(mut receiver: watch::Receiver<Option<ScanRequest>>, scan: S, report: R)
where
    S: Fn(&ScanRequest) -> io::Result<Vec<PathBuf>>,
    R: Fn(&ScanRequest, io::Result<Vec<PathBuf>>),
{
    while executor::block_on(receiver.changed()).is_ok() {
        let Some(request) = receiver.borrow_and_update().clone() else {
            continue;
        };
        if receiver.has_changed().is_err() {
            break;
        }
        let result = scan(&request);
        let is_latest = receiver.has_changed().is_ok()
            && receiver
                .borrow()
                .as_ref()
                .is_some_and(|latest| latest.generation == request.generation);
        if is_latest {
            report(&request, result);
        }
    }
}

fn scan_orphaned_disk_buffers(request: &ScanRequest) -> io::Result<Vec<PathBuf>> {
    find_orphaned_disk_buffers(&request.data_dir, &request.configured_buffer_paths)
}

fn report_orphaned_disk_buffers(request: &ScanRequest, result: io::Result<Vec<PathBuf>>) {
    let orphaned_buffers = match result {
        Ok(orphaned_buffers) => orphaned_buffers,
        Err(error) => {
            warn!(
                data_dir = request.data_dir.to_string_lossy().as_ref(),
                %error,
                message = "Failed to scan for unreferenced disk buffers.",
            );
            return;
        }
    };

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
        warn!(
            buffer_id = orphaned_buffer_id.to_string_lossy().as_ref(),
            buffer_dir = orphaned_buffer.to_string_lossy().as_ref(),
            message = "Found disk buffer not referenced by the current configuration; any remaining data is not being processed by the current topology.",
        );
    }
}

fn referenced_disk_buffer_directories(config: &Config) -> HashSet<PathBuf> {
    let data_dir = config.global.data_dir.clone();
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
    data_dir: Option<PathBuf>,
    buffers: impl Iterator<Item = (ComponentKey, BufferConfig)>,
) -> HashSet<PathBuf> {
    buffers
        .flat_map(|(id, buffer)| {
            buffer
                .stages()
                .iter()
                .filter_map(|stage| stage.disk_usage(data_dir.clone(), &id))
                .map(|usage| usage.data_dir().to_path_buf())
                .collect::<Vec<_>>()
        })
        .collect()
}

fn find_orphaned_disk_buffers(
    data_dir: &Path,
    configured_buffer_paths: &HashSet<PathBuf>,
) -> io::Result<Vec<PathBuf>> {
    let disk_buffer_root = data_dir.join("buffer").join("v2");
    let canonical_root = match fs::canonicalize(&disk_buffer_root) {
        Ok(root) => root,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };
    let configured_canonical_paths = configured_buffer_paths
        .iter()
        .filter_map(|path| fs::canonicalize(path).ok())
        .collect::<HashSet<_>>();

    let mut pending = vec![disk_buffer_root.clone()];
    let mut visited = HashSet::new();
    let mut orphaned_buffers = Vec::new();
    while let Some(path) = pending.pop() {
        let Some(canonical_path) = canonical_path_within_root(&path, &canonical_root) else {
            continue;
        };
        if !visited.insert(canonical_path.clone()) {
            continue;
        }

        let entries = match fs::read_dir(&path) {
            Ok(entries) => entries,
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
            let metadata = match fs::metadata(&entry_path) {
                Ok(metadata) => metadata,
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
        if path != disk_buffer_root
            && has_disk_buffer_file
            && !configured_buffer_paths.contains(&path)
            && !configured_canonical_paths.contains(&canonical_path)
        {
            orphaned_buffers.push(path);
        }
        child_directories.sort_by(|left, right| right.cmp(left));
        pending.extend(child_directories);
    }
    orphaned_buffers.sort();
    Ok(orphaned_buffers)
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
    match fs::canonicalize(path) {
        Ok(path) if path.starts_with(canonical_root) => Some(path),
        Ok(target) => {
            warn!(
                path = path.to_string_lossy().as_ref(),
                target = target.to_string_lossy().as_ref(),
                message = "Skipping disk buffer path outside the buffer root.",
            );
            None
        }
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

#[cfg(test)]
mod tests {
    use std::{
        collections::HashSet,
        fs,
        num::NonZeroU64,
        path::PathBuf,
        sync::{Arc, Mutex, mpsc},
        time::Duration,
    };

    #[cfg(unix)]
    use std::os::unix::fs::symlink;

    use tempfile::tempdir;
    use vector_lib::{
        buffers::{BufferConfig, BufferType},
        config::ComponentKey,
    };

    use super::{
        OrphanedDiskBufferScanner, ScanRequest, disk_buffer_directories,
        find_orphaned_disk_buffers, report_orphaned_disk_buffers,
    };

    #[cfg(feature = "enrichment-tables-memory")]
    use super::referenced_disk_buffer_directories;

    #[cfg(feature = "enrichment-tables-memory")]
    use crate::{
        config::{Config, unit_test::UnitTestSourceConfig},
        enrichment_tables::{EnrichmentTables, memory::MemoryConfig},
        test_util::mock::basic_sink,
    };

    fn recv<T>(receiver: &mpsc::Receiver<T>) -> T {
        receiver.recv_timeout(Duration::from_secs(2)).unwrap()
    }

    #[test]
    fn scanner_coalesces_requests_and_suppresses_stale_results() {
        let (started_tx, started_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let release_rx = Arc::new(Mutex::new(release_rx));
        let (reported_tx, reported_rx) = mpsc::channel();
        let scanner = OrphanedDiskBufferScanner::spawn(
            move |request| {
                started_tx.send(request.data_dir.clone()).unwrap();
                release_rx.lock().unwrap().recv().unwrap();
                Ok(vec![request.data_dir.clone()])
            },
            move |request, _| reported_tx.send(request.data_dir.clone()).unwrap(),
        );

        scanner.schedule("first".into(), HashSet::new());
        assert_eq!(recv(&started_rx), PathBuf::from("first"));
        scanner.schedule("superseded".into(), HashSet::new());
        scanner.schedule("latest".into(), HashSet::new());
        release_tx.send(()).unwrap();

        assert_eq!(recv(&started_rx), PathBuf::from("latest"));
        assert!(reported_rx.try_recv().is_err());
        release_tx.send(()).unwrap();
        assert_eq!(recv(&reported_rx), PathBuf::from("latest"));
        assert!(started_rx.try_recv().is_err());
    }

    #[test]
    fn scanner_schedule_and_drop_are_nonblocking_and_worker_terminates() {
        let (release_tx, release_rx) = mpsc::channel();
        let release_rx = Arc::new(Mutex::new(release_rx));
        let (started_tx, started_rx) = mpsc::channel();
        let (worker_tx, worker_rx) = mpsc::channel::<()>();
        let scanner = OrphanedDiskBufferScanner::spawn(
            move |_| {
                started_tx.send(()).unwrap();
                release_rx.lock().unwrap().recv().unwrap();
                Ok(Vec::new())
            },
            move |_, _| drop(worker_tx.clone()),
        );
        scanner.schedule("scan".into(), HashSet::new());
        recv(&started_rx);

        scanner.schedule("latest".into(), HashSet::new());
        drop(scanner);
        release_tx.send(()).unwrap();

        assert!(matches!(
            worker_rx.recv_timeout(Duration::from_secs(2)),
            Err(mpsc::RecvTimeoutError::Disconnected)
        ));
        assert!(started_rx.try_recv().is_err());
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

        let started = std::time::Instant::now();
        drop(runtime);
        assert!(started.elapsed() < Duration::from_millis(250));
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

    #[test]
    fn missing_disk_buffer_root_has_no_orphans() {
        let data_dir = tempdir().unwrap();
        let actual = find_orphaned_disk_buffers(data_dir.path(), &HashSet::new()).unwrap();
        assert!(actual.is_empty());
    }

    #[test]
    fn reporter_skips_results_outside_the_buffer_root() {
        let request = ScanRequest {
            generation: 1,
            data_dir: PathBuf::from("data"),
            configured_buffer_paths: HashSet::new(),
        };

        report_orphaned_disk_buffers(&request, Ok(vec![PathBuf::from("elsewhere")]));
    }

    #[test]
    fn disk_path_collection_includes_all_sink_entries() {
        let data_dir = PathBuf::from("data");
        let disk_buffer = || {
            BufferConfig::Single(BufferType::DiskV2 {
                max_size: NonZeroU64::new(1).unwrap(),
                when_full: Default::default(),
            })
        };
        let sinks = [
            (ComponentKey::from("ordinary"), disk_buffer()),
            (ComponentKey::from("enrichment"), disk_buffer()),
        ];

        let actual = disk_buffer_directories(Some(data_dir.clone()), sinks.into_iter());

        assert_eq!(
            actual,
            HashSet::from([
                data_dir.join("buffer/v2/ordinary"),
                data_dir.join("buffer/v2/enrichment"),
            ])
        );
    }

    #[cfg(feature = "enrichment-tables-memory")]
    #[test]
    fn configured_paths_are_discovered_from_real_sink_and_enrichment_table_config() {
        let data_dir = PathBuf::from("data");
        let ordinary_key = ComponentKey::from("ordinary");
        let enrichment_key = ComponentKey::from("enrichment");
        let mut builder = Config::builder();
        builder.global.data_dir = Some(data_dir.clone());
        builder.add_source("in", UnitTestSourceConfig::default());
        builder.add_sink("ordinary", &["in"], basic_sink(1).1);
        builder.sinks[&ordinary_key].buffer = BufferConfig::Single(BufferType::DiskV2 {
            max_size: NonZeroU64::new(268_435_488).unwrap(),
            when_full: Default::default(),
        });
        builder.add_enrichment_table(
            "enrichment",
            &["in"],
            EnrichmentTables::Memory(MemoryConfig::default()),
        );
        let config = builder.build().unwrap();

        // Enrichment-table sinks currently always receive the default memory buffer in `as_sink`,
        // so no real configuration can assign one a disk stage. This still exercises that iterator
        // alongside an ordinary disk-buffered sink at the Config boundary.
        assert!(
            config
                .enrichment_table(&enrichment_key)
                .unwrap()
                .as_sink(&enrichment_key)
                .is_some()
        );
        assert_eq!(
            referenced_disk_buffer_directories(&config),
            HashSet::from([data_dir.join("buffer/v2/ordinary")])
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

    #[cfg(unix)]
    #[test]
    fn matches_configured_buffers_by_symlink_target() {
        let data_dir = tempdir().unwrap();
        let buffer_root = data_dir.path().join("buffer").join("v2");
        let target = buffer_root.join("z-target");
        let configured_link = buffer_root.join("a-configured-link");
        fs::create_dir_all(&target).unwrap();
        fs::write(target.join("buffer.db"), b"configured").unwrap();
        symlink(&target, &configured_link).unwrap();

        let configured_paths = HashSet::from([configured_link]);
        let actual = find_orphaned_disk_buffers(data_dir.path(), &configured_paths).unwrap();
        assert!(actual.is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn supports_a_symlinked_disk_buffer_root() {
        let data_dir = tempdir().unwrap();
        let storage_dir = tempdir().unwrap();
        let buffer_parent = data_dir.path().join("buffer");
        let buffer_root = buffer_parent.join("v2");
        let orphaned = buffer_root.join("orphaned");
        fs::create_dir(&buffer_parent).unwrap();
        fs::create_dir(storage_dir.path().join("orphaned")).unwrap();
        fs::write(
            storage_dir.path().join("orphaned").join("buffer.db"),
            b"orphaned",
        )
        .unwrap();
        symlink(storage_dir.path(), &buffer_root).unwrap();

        let actual = find_orphaned_disk_buffers(data_dir.path(), &HashSet::new()).unwrap();
        assert_eq!(actual, vec![orphaned]);
    }
}
