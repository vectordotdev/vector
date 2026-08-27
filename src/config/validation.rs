use std::{
    collections::{HashMap, HashSet},
    fs, io,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
        mpsc::{self, SyncSender},
    },
};

use futures_util::{FutureExt, StreamExt, TryFutureExt, TryStreamExt, stream};
use heim::{disk::Partition, units::information::byte};
use indexmap::IndexMap;
use vector_lib::{buffers::config::DiskUsage, internal_event::DEFAULT_OUTPUT};

use super::{
    ComponentKey, Config, OutputId, Resource, builder::ConfigBuilder,
    transform::get_transform_output_ids,
};

/// Minimum value (exclusive) for EWMA alpha options.
/// The alpha value must be strictly greater than this value.
const EWMA_ALPHA_MIN: f64 = 0.0;

/// Maximum value (exclusive) for EWMA alpha options.
/// The alpha value must be strictly less than this value.
const EWMA_ALPHA_MAX: f64 = 1.0;

/// Minimum value (exclusive) for EWMA half-life options.
/// The half-life value must be strictly greater than this value.
const EWMA_HALF_LIFE_SECONDS_MIN: f64 = 0.0;

/// Validates an optional EWMA alpha value and returns an error message if invalid.
/// Returns `None` if the value is `None` or valid, otherwise returns an error message.
fn validate_ewma_alpha(alpha: Option<f64>, field_name: &str) -> Option<String> {
    if let Some(alpha) = alpha
        && !(alpha > EWMA_ALPHA_MIN && alpha < EWMA_ALPHA_MAX)
    {
        Some(format!(
            "Global `{field_name}` must be between 0 and 1 exclusive (0 < alpha < 1), got {alpha}"
        ))
    } else {
        None
    }
}

/// Validates an optional EWMA half-life value and returns an error message if invalid.
/// Returns `None` if the value is `None` or valid, otherwise returns an error message.
#[expect(
    clippy::neg_cmp_op_on_partial_ord,
    reason = "!(x > 0) rejects NaN and non-positive values; (x <= 0) would incorrectly accept NaN"
)]
fn validate_ewma_half_life_seconds(
    half_life_seconds: Option<f64>,
    field_name: &str,
) -> Option<String> {
    if let Some(half_life_seconds) = half_life_seconds
        && !(half_life_seconds > EWMA_HALF_LIFE_SECONDS_MIN)
    {
        Some(format!(
            "Global `{field_name}` must be greater than 0, got {half_life_seconds}"
        ))
    } else {
        None
    }
}

/// Check that provide + topology config aren't present in the same builder, which is an error.
pub fn check_provider(config: &ConfigBuilder) -> Result<(), Vec<String>> {
    if config.provider.is_some()
        && (!config.sources.is_empty() || !config.transforms.is_empty() || !config.sinks.is_empty())
    {
        Err(vec![
            "No sources/transforms/sinks are allowed if provider config is present.".to_owned(),
        ])
    } else {
        Ok(())
    }
}

pub fn check_names<'a, I: Iterator<Item = &'a ComponentKey>>(names: I) -> Result<(), Vec<String>> {
    let errors: Vec<_> = names
        .filter(|component_key| component_key.id().contains('.'))
        .map(|component_key| {
            format!(
                "Component name \"{}\" should not contain a \".\"",
                component_key.id()
            )
        })
        .collect();

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

pub fn check_shape(config: &ConfigBuilder) -> Result<(), Vec<String>> {
    let mut errors = vec![];

    if !config.allow_empty {
        if config.sources.is_empty() {
            errors.push("No sources defined in the config.".to_owned());
        }

        if config.sinks.is_empty() {
            errors.push("No sinks defined in the config.".to_owned());
        }
    }

    // Helper for below
    fn tagged<'a>(
        tag: &'static str,
        iter: impl Iterator<Item = &'a ComponentKey>,
    ) -> impl Iterator<Item = (&'static str, &'a ComponentKey)> {
        iter.map(move |x| (tag, x))
    }

    // Check for non-unique names across sources, sinks, and transforms
    let mut used_keys = HashMap::<&ComponentKey, Vec<&'static str>>::new();
    for (ctype, id) in tagged("source", config.sources.keys())
        .chain(tagged("transform", config.transforms.keys()))
        .chain(tagged("sink", config.sinks.keys()))
    {
        let uses = used_keys.entry(id).or_default();
        uses.push(ctype);
    }

    for (id, uses) in used_keys.into_iter().filter(|(_id, uses)| uses.len() > 1) {
        errors.push(format!(
            "More than one component with name \"{}\" ({}).",
            id,
            uses.join(", ")
        ));
    }

    // Warnings and errors
    let sink_inputs = config
        .sinks
        .iter()
        .map(|(key, sink)| ("sink", key.clone(), sink.inputs.clone()));
    let transform_inputs = config
        .transforms
        .iter()
        .map(|(key, transform)| ("transform", key.clone(), transform.inputs.clone()));
    for (output_type, key, inputs) in sink_inputs.chain(transform_inputs) {
        if inputs.is_empty() {
            errors.push(format!(
                "{} \"{}\" has no inputs",
                capitalize(output_type),
                key
            ));
        }

        let mut frequencies = HashMap::new();
        for input in inputs {
            let entry = frequencies.entry(input).or_insert(0usize);
            *entry += 1;
        }

        for (dup, count) in frequencies.into_iter().filter(|(_name, count)| *count > 1) {
            errors.push(format!(
                "{} \"{}\" has input \"{}\" duplicated {} times",
                capitalize(output_type),
                key,
                dup,
                count,
            ));
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

pub fn check_resources(config: &ConfigBuilder) -> Result<(), Vec<String>> {
    let source_resources = config
        .sources
        .iter()
        .map(|(id, config)| (id, config.inner.resources()));
    let sink_resources = config
        .sinks
        .iter()
        .map(|(id, config)| (id, config.resources(id)));

    let conflicting_components = Resource::conflicts(source_resources.chain(sink_resources));

    if conflicting_components.is_empty() {
        Ok(())
    } else {
        Err(conflicting_components
            .into_iter()
            .map(|(resource, components)| {
                format!("Resource `{resource}` is claimed by multiple components: {components:?}")
            })
            .collect())
    }
}

/// Validates that `*_ewma_alpha` values are within the valid range (0 < alpha < 1).
pub fn check_values(config: &ConfigBuilder) -> Result<(), Vec<String>> {
    let mut errors = Vec::new();

    if let Some(error) = validate_ewma_half_life_seconds(
        config.global.buffer_utilization_ewma_half_life_seconds,
        "buffer_utilization_ewma_half_life_seconds",
    ) {
        errors.push(error);
    }
    if let Some(error) = validate_ewma_alpha(config.global.latency_ewma_alpha, "latency_ewma_alpha")
    {
        errors.push(error);
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

/// To avoid collisions between `output` metric tags, check that a component
/// does not have a named output with the name [`DEFAULT_OUTPUT`]
pub fn check_outputs(config: &ConfigBuilder) -> Result<(), Vec<String>> {
    let mut errors = Vec::new();
    for (key, source) in config.sources.iter() {
        let outputs = source.inner.outputs(config.schema.log_namespace());
        if outputs
            .iter()
            .map(|output| output.port.as_deref().unwrap_or(""))
            .any(|name| name == DEFAULT_OUTPUT)
        {
            errors.push(format!(
                "Source {key} cannot have a named output with reserved name: `{DEFAULT_OUTPUT}`"
            ));
        }
    }

    for (key, transform) in config.transforms.iter() {
        // Structural validation: reserved names, duplicate routes, invalid sample rates.
        // These checks run during config compilation. Transforms that need the schema/enrichment
        // context must implement validate_with_context(), called later in validate.rs.
        if let Err(errs) = transform.inner.validate_structure() {
            errors.extend(errs.into_iter().map(|msg| format!("Transform {key} {msg}")));
        }

        if get_transform_output_ids(
            transform.inner.as_ref(),
            key.clone(),
            config.schema.log_namespace(),
        )
        .any(|output| matches!(output.port, Some(output) if output == DEFAULT_OUTPUT))
        {
            errors.push(format!(
                "Transform {key} cannot have a named output with reserved name: `{DEFAULT_OUTPUT}`"
            ));
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

pub async fn check_buffer_preconditions(config: &Config) -> Result<(), Vec<String>> {
    // We need to assert that Vector's data directory is located on a mountpoint that has enough
    // capacity to allow all sinks with disk buffers configured to be able to use up to their
    // maximum configured size without overrunning the total capacity.
    //
    // More subtly, we need to make sure we properly map a given buffer's data directory to the
    // appropriate mountpoint, as it is technically possible that individual buffers could be on
    // separate mountpoints.
    //
    // Notably, this does *not* cover other data usage by Vector on the same mountpoint because we
    // don't always know the upper bound of that usage i.e. file checkpoint state.

    // Grab all configured disk buffers, and if none are present, simply return early.
    let global_data_dir = config.global.data_dir.clone();
    let configured_disk_buffers = config
        .sinks()
        .flat_map(|(id, sink)| {
            sink.buffer
                .stages()
                .iter()
                .filter_map(|stage| stage.disk_usage(global_data_dir.clone(), id))
        })
        .collect::<Vec<_>>();

    if configured_disk_buffers.is_empty() {
        return Ok(());
    }

    // Now query all the mountpoints on the system, and get their total capacity. We also have to
    // sort the mountpoints from longest to shortest so we can find the longest prefix match for
    // each buffer data directory by simply iterating from beginning to end.
    let mountpoints = heim::disk::partitions()
        .and_then(|stream| stream.try_collect::<Vec<_>>().and_then(process_partitions))
        .or_else(|_| {
            heim::disk::partitions_physical()
                .and_then(|stream| stream.try_collect::<Vec<_>>().and_then(process_partitions))
        })
        .await;

    let mountpoints = match mountpoints {
        Ok(mut mountpoints) => {
            mountpoints.sort_by(|m1, _, m2, _| m2.cmp(m1));
            mountpoints
        }
        Err(e) => {
            warn!(
                cause = %e,
                message = "Failed to query disk partitions. Cannot ensure that buffer size limits are within physical storage capacity limits.",
            );
            return Ok(());
        }
    };

    // Now build a mapping of buffer IDs/usage configuration to the mountpoint they reside on.
    let mountpoint_buffer_mapping = configured_disk_buffers.into_iter().fold(
        HashMap::new(),
        |mut mappings: HashMap<PathBuf, Vec<DiskUsage>>, usage| {
            let canonicalized_data_dir = usage
                .data_dir()
                .canonicalize()
                .unwrap_or_else(|_| usage.data_dir().to_path_buf());
            let mountpoint = mountpoints
                .keys()
                .find(|mountpoint| canonicalized_data_dir.starts_with(mountpoint));

            match mountpoint {
                None => warn!(
                    buffer_id = usage.id().id(),
                    data_dir = usage.data_dir().to_string_lossy().as_ref(),
                    canonicalized_data_dir = canonicalized_data_dir.to_string_lossy().as_ref(),
                    message = "Found no matching mountpoint for buffer data directory.",
                ),
                Some(mountpoint) => {
                    mappings.entry(mountpoint.clone()).or_default().push(usage);
                }
            }

            mappings
        },
    );

    // Finally, we have a mapping of disk buffers, based on their underlying mountpoint. Go through
    // and check to make sure the sum total of `max_size` for all buffers associated with each
    // mountpoint does not exceed that mountpoint's total capacity.
    //
    // We specifically do not do any sort of warning on free space because that has to be the
    // responsibility of the operator to ensure there's enough total space for all buffers present.
    let mut errors = Vec::new();

    for (mountpoint, buffers) in mountpoint_buffer_mapping {
        let buffer_max_size_total: u64 = buffers.iter().map(|usage| usage.max_size()).sum();
        let mountpoint_total_capacity = mountpoints
            .get(&mountpoint)
            .copied()
            .expect("mountpoint must exist");

        if buffer_max_size_total > mountpoint_total_capacity {
            let component_ids = buffers
                .iter()
                .map(|usage| usage.id().id())
                .collect::<Vec<_>>();
            errors.push(format!(
                "Mountpoint '{}' has total capacity of {} bytes, but configured buffers using mountpoint have total maximum size of {} bytes. \
Reduce the `max_size` of the buffers to fit within the total capacity of the mountpoint. (components associated with mountpoint: {})",
                mountpoint.to_string_lossy(), mountpoint_total_capacity, buffer_max_size_total, component_ids.join(", "),
            ));
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

async fn process_partitions(partitions: Vec<Partition>) -> heim::Result<IndexMap<PathBuf, u64>> {
    stream::iter(partitions)
        .map(Ok)
        .and_then(|partition| {
            let mountpoint_path = partition.mount_point().to_path_buf();
            heim::disk::usage(mountpoint_path.clone())
                .map(|usage| usage.map(|usage| (mountpoint_path, usage.total().get::<byte>())))
        })
        .try_collect::<IndexMap<_, _>>()
        .await
}

// Scan for unreferenced disk buffers on a dedicated thread so blocking filesystem
// operations cannot delay the async runtime. While a scan is running, retain only
// the latest reload request because the results are diagnostic only.
pub(crate) struct OrphanedDiskBufferScanner {
    sender: SyncSender<()>,
    pending: Arc<Mutex<Option<ScanRequest>>>,
    generation: Arc<AtomicU64>,
}

struct ScanRequest {
    generation: u64,
    data_dir: PathBuf,
    configured_buffer_paths: HashSet<PathBuf>,
}

impl OrphanedDiskBufferScanner {
    pub(crate) fn new() -> Self {
        Self::spawn(scan_orphaned_disk_buffers, report_orphaned_disk_buffers)
    }

    fn spawn<S, R>(scan: S, report: R) -> Self
    where
        S: Fn(&ScanRequest) -> io::Result<Vec<PathBuf>> + Send + 'static,
        R: Fn(&ScanRequest, io::Result<Vec<PathBuf>>) + Send + 'static,
    {
        let (sender, receiver) = mpsc::sync_channel(1);
        let pending = Arc::new(Mutex::new(None));
        let generation = Arc::new(AtomicU64::new(0));
        let worker_pending = Arc::clone(&pending);
        let worker_generation = Arc::clone(&generation);
        if let Err(error) = std::thread::Builder::new()
            .name("orphaned-disk-buffer-scanner".into())
            .spawn(move || run_scanner(receiver, worker_pending, worker_generation, scan, report))
        {
            warn!(%error, message = "Failed to start unreferenced disk buffer scanner.");
        }
        Self {
            sender,
            pending,
            generation,
        }
    }

    pub(crate) fn scan(&self, config: &Config, temporary_exclusions: HashSet<PathBuf>) {
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
        let mut pending = self.pending.lock().expect("scanner mutex poisoned");
        let generation = self
            .generation
            .fetch_add(1, Ordering::AcqRel)
            .wrapping_add(1);
        *pending = Some(ScanRequest {
            generation,
            data_dir,
            configured_buffer_paths,
        });
        drop(pending);
        let _send_result = self.sender.try_send(());
    }
}

impl Drop for OrphanedDiskBufferScanner {
    fn drop(&mut self) {
        self.generation.fetch_add(1, Ordering::Release);
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
        let orphaned_buffer_id = orphaned_buffer
            .strip_prefix(&disk_buffer_root)
            .expect("orphaned buffer must be under the disk buffer root");
        warn!(
            buffer_id = orphaned_buffer_id.to_string_lossy().as_ref(),
            buffer_dir = orphaned_buffer.to_string_lossy().as_ref(),
            message = "Found disk buffer not referenced by the current configuration; any data in it will not be delivered by the current topology.",
        );
    }
}

fn run_scanner<S, R>(
    receiver: mpsc::Receiver<()>,
    pending: Arc<Mutex<Option<ScanRequest>>>,
    generation: Arc<AtomicU64>,
    scan: S,
    report: R,
) where
    S: Fn(&ScanRequest) -> io::Result<Vec<PathBuf>>,
    R: Fn(&ScanRequest, io::Result<Vec<PathBuf>>),
{
    while receiver.recv().is_ok() {
        let request = pending.lock().expect("scanner mutex poisoned").take();
        let Some(request) = request else {
            continue;
        };
        let result = scan(&request);
        if generation.load(Ordering::Acquire) == request.generation {
            report(&request, result);
        }
    }
}

pub(crate) fn referenced_disk_buffer_directories(config: &Config) -> HashSet<PathBuf> {
    let data_dir = config.global.data_dir.clone();
    config
        .sinks()
        .flat_map(|(id, sink)| {
            sink.buffer
                .stages()
                .iter()
                .filter_map(|stage| stage.disk_usage(data_dir.clone(), id))
        })
        .map(|usage| usage.data_dir().to_path_buf())
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
            } else if metadata.is_file() && is_disk_buffer_file(&entry.file_name()) {
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

fn is_disk_buffer_file(file_name: &std::ffi::OsStr) -> bool {
    if file_name == "buffer.db" {
        return true;
    }
    file_name
        .to_str()
        .and_then(|name| name.strip_prefix("buffer-data-"))
        .and_then(|name| name.strip_suffix(".dat"))
        .and_then(|file_id| file_id.parse::<u16>().ok())
        .is_some_and(|file_id| file_id < u16::MAX)
}

pub fn warnings(config: &Config) -> Vec<String> {
    let mut warnings = vec![];

    let table_sources = config
        .enrichment_tables
        .iter()
        .filter_map(|(key, table)| table.as_source(key))
        .collect::<Vec<_>>();
    let source_ids = config
        .sources
        .iter()
        .chain(table_sources.iter().map(|(k, s)| (k, s)))
        .flat_map(|(key, source)| {
            source
                .inner
                .outputs(config.schema.log_namespace())
                .iter()
                .map(|output| {
                    if let Some(port) = &output.port {
                        ("source", OutputId::from((key, port.clone())))
                    } else {
                        ("source", OutputId::from(key))
                    }
                })
                .collect::<Vec<_>>()
        });
    let transform_ids = config.transforms.iter().flat_map(|(key, transform)| {
        get_transform_output_ids(
            transform.inner.as_ref(),
            key.clone(),
            config.schema.log_namespace(),
        )
        .map(|output| ("transform", output))
        .collect::<Vec<_>>()
    });

    let table_sinks = config
        .enrichment_tables
        .iter()
        .filter_map(|(key, table)| table.as_sink(key))
        .collect::<Vec<_>>();
    for (input_type, id) in transform_ids.chain(source_ids) {
        if !config
            .transforms
            .iter()
            .any(|(_, transform)| transform.inputs.contains(&id))
            && !config
                .sinks
                .iter()
                .any(|(_, sink)| sink.inputs.contains(&id))
            && !table_sinks
                .iter()
                .any(|(_, sink)| sink.inputs.contains(&id))
        {
            warnings.push(format!(
                "{} \"{}\" has no consumers",
                capitalize(input_type),
                id
            ));
        }
    }

    warnings
}

fn capitalize(s: &str) -> String {
    let mut s = s.to_owned();
    if let Some(r) = s.get_mut(0..1) {
        r.make_ascii_uppercase();
    }
    s
}

#[cfg(test)]
mod tests {
    use std::{
        collections::HashSet,
        fs,
        path::PathBuf,
        sync::{Arc, mpsc},
        thread,
        time::Duration,
    };

    #[cfg(unix)]
    use std::os::unix::fs::symlink;

    use tempfile::tempdir;

    use super::{OrphanedDiskBufferScanner, find_orphaned_disk_buffers};

    fn recv<T>(receiver: &mpsc::Receiver<T>) -> T {
        receiver.recv_timeout(Duration::from_secs(1)).unwrap()
    }

    #[test]
    fn scanner_coalesces_requests_and_suppresses_stale_results() {
        let (started_tx, started_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let (reported_tx, reported_rx) = mpsc::channel();
        let scanner = OrphanedDiskBufferScanner::spawn(
            move |request| {
                started_tx.send(request.data_dir.clone()).unwrap();
                release_rx.recv().unwrap();
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
    fn scanner_schedule_and_drop_do_not_wait_for_reporter() {
        let (report_started_tx, report_started_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let scanner = Arc::new(OrphanedDiskBufferScanner::spawn(
            |request| Ok(vec![request.data_dir.clone()]),
            move |_, _| {
                report_started_tx.send(()).unwrap();
                release_rx.recv().unwrap();
            },
        ));
        scanner.schedule("scan".into(), HashSet::new());
        recv(&report_started_rx);

        let schedule_scanner = Arc::clone(&scanner);
        let (scheduled_tx, scheduled_rx) = mpsc::channel();
        thread::spawn(move || {
            schedule_scanner.schedule("latest".into(), HashSet::new());
            drop(schedule_scanner);
            scheduled_tx.send(()).unwrap();
        });
        recv(&scheduled_rx);

        let (dropped_tx, dropped_rx) = mpsc::channel();
        thread::spawn(move || {
            drop(scanner);
            dropped_tx.send(()).unwrap();
        });
        recv(&dropped_rx);
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
