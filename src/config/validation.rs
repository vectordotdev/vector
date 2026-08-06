use std::{
    collections::{HashMap, HashSet},
    fs, io,
    path::{Path, PathBuf},
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

#[derive(Clone, Copy)]
struct MountpointDiskUsage {
    total: u64,
    available: u64,
}

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
        .chain(
            config
                .enrichment_tables()
                .filter_map(|(id, table)| table.as_sink(id))
                .flat_map(|(id, sink)| {
                    sink.buffer
                        .stages()
                        .iter()
                        .filter_map(|stage| stage.disk_usage(global_data_dir.clone(), &id))
                        .collect::<Vec<_>>()
                }),
        )
        .collect::<Vec<_>>();

    if let Some(global_data_dir) = global_data_dir.as_deref() {
        let configured_buffer_paths = configured_disk_buffers
            .iter()
            .map(|usage| usage.data_dir().to_path_buf())
            .collect::<HashSet<_>>();

        match find_orphaned_disk_buffers(global_data_dir, &configured_buffer_paths) {
            Ok(orphaned_buffers) => {
                for orphaned_buffer in orphaned_buffers {
                    match directory_allocated_size(&orphaned_buffer) {
                        Ok(allocated_bytes) => warn!(
                            buffer_dir = orphaned_buffer.to_string_lossy().as_ref(),
                            allocated_bytes,
                            message = "Found a disk buffer not referenced by the new configuration. It may still be draining from the previous configuration during reload, but Vector will not reopen it on the next startup unless its component is restored.",
                        ),
                        Err(error) => warn!(
                            buffer_dir = orphaned_buffer.to_string_lossy().as_ref(),
                            %error,
                            message = "Found a disk buffer not referenced by the new configuration, but failed to determine its size. It may still be draining during reload, but Vector will not reopen it on the next startup unless its component is restored.",
                        ),
                    }
                }
            }
            Err(error) => warn!(
                data_dir = global_data_dir.to_string_lossy().as_ref(),
                %error,
                message = "Failed to inspect the disk buffer directory for orphaned buffers.",
            ),
        }
    }

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

    // Finally, check both the total capacity and the capacity that the configured buffers can
    // actually grow into. Existing files belonging to configured buffers are recyclable, while
    // unrelated files and orphaned buffers are not.
    let mut errors = Vec::new();

    for (mountpoint, buffers) in mountpoint_buffer_mapping {
        let buffer_max_size_total = buffers
            .iter()
            .fold(0u64, |total, usage| total.saturating_add(usage.max_size()));
        let mountpoint_usage = mountpoints
            .get(&mountpoint)
            .copied()
            .expect("mountpoint must exist");

        let component_ids = buffers
            .iter()
            .map(|usage| usage.id().id())
            .collect::<Vec<_>>();

        if buffer_max_size_total > mountpoint_usage.total {
            errors.push(format!(
                "Mountpoint '{}' has total capacity of {} bytes, but configured buffers using mountpoint have total maximum size of {} bytes. \
Reduce the `max_size` of the buffers to fit within the total capacity of the mountpoint. (components associated with mountpoint: {})",
                mountpoint.to_string_lossy(), mountpoint_usage.total, buffer_max_size_total, component_ids.join(", "),
            ));
            continue;
        }

        let configured_buffer_allocated_bytes = buffers.iter().fold(0u64, |total, usage| {
            match directory_allocated_size(usage.data_dir()) {
                Ok(size) => total.saturating_add(size),
                Err(error) if error.kind() == io::ErrorKind::NotFound => total,
                Err(error) => {
                    warn!(
                        buffer_id = usage.id().id(),
                        buffer_dir = usage.data_dir().to_string_lossy().as_ref(),
                        %error,
                        message = "Failed to determine current disk buffer size. Treating it as zero for available-space validation.",
                    );
                    total
                }
            }
        });
        let available_bytes = match heim::disk::usage(&mountpoint).await {
            Ok(usage) => mountpoint_usage.available.min(usage.free().get::<byte>()),
            Err(error) => {
                warn!(
                    mountpoint = mountpoint.to_string_lossy().as_ref(),
                    %error,
                    message = "Failed to refresh available disk space after measuring configured buffers. Using the initial measurement.",
                );
                mountpoint_usage.available
            }
        };

        if !has_sufficient_buffer_capacity(
            buffer_max_size_total,
            available_bytes,
            configured_buffer_allocated_bytes,
        ) {
            errors.push(format!(
                "Mountpoint '{}' has {} bytes available and {} bytes currently allocated to configured buffers, but those buffers have a total maximum size of {} bytes. \
Free disk space, remove orphaned buffers after confirming their data is no longer needed, or reduce the configured `max_size`. (components associated with mountpoint: {})",
                mountpoint.to_string_lossy(),
                available_bytes,
                configured_buffer_allocated_bytes,
                buffer_max_size_total,
                component_ids.join(", "),
            ));
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

const fn has_sufficient_buffer_capacity(
    maximum_buffer_bytes: u64,
    available_bytes: u64,
    configured_buffer_allocated_bytes: u64,
) -> bool {
    maximum_buffer_bytes <= available_bytes.saturating_add(configured_buffer_allocated_bytes)
}

async fn process_partitions(
    partitions: Vec<Partition>,
) -> heim::Result<IndexMap<PathBuf, MountpointDiskUsage>> {
    stream::iter(partitions)
        .map(Ok)
        .and_then(|partition| {
            let mountpoint_path = partition.mount_point().to_path_buf();
            heim::disk::usage(mountpoint_path.clone()).map(|usage| {
                usage.map(|usage| {
                    (
                        mountpoint_path,
                        MountpointDiskUsage {
                            total: usage.total().get::<byte>(),
                            available: usage.free().get::<byte>(),
                        },
                    )
                })
            })
        })
        .try_collect::<IndexMap<_, _>>()
        .await
}

fn find_orphaned_disk_buffers(
    data_dir: &Path,
    configured_buffer_paths: &HashSet<PathBuf>,
) -> io::Result<Vec<PathBuf>> {
    let disk_buffer_root = data_dir.join("buffer").join("v2");
    let entries = match fs::read_dir(disk_buffer_root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };

    let mut pending = Vec::new();
    for entry in entries {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            pending.push(entry.path());
        }
    }

    let mut orphaned_buffers = Vec::new();
    while let Some(path) = pending.pop() {
        if configured_buffer_paths.contains(&path) {
            continue;
        }

        if configured_buffer_paths
            .iter()
            .any(|configured_path| configured_path.starts_with(&path))
        {
            for entry in fs::read_dir(path)? {
                let entry = entry?;
                if entry.file_type()?.is_dir() {
                    pending.push(entry.path());
                }
            }
        } else {
            orphaned_buffers.push(path);
        }
    }
    orphaned_buffers.sort();
    Ok(orphaned_buffers)
}

fn directory_allocated_size(path: &Path) -> io::Result<u64> {
    let mut total = 0u64;
    let mut pending = vec![path.to_path_buf()];

    while let Some(path) = pending.pop() {
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            let file_type = match entry.file_type() {
                Ok(file_type) => file_type,
                Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
                Err(error) => return Err(error),
            };
            if file_type.is_symlink() {
                continue;
            }
            let metadata = match entry.metadata() {
                Ok(metadata) => metadata,
                Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
                Err(error) => return Err(error),
            };
            if file_type.is_dir() {
                pending.push(entry.path());
            } else if file_type.is_file() {
                total = total.saturating_add(file_allocated_size(&metadata));
            }
        }
    }

    Ok(total)
}

#[cfg(unix)]
fn file_allocated_size(metadata: &fs::Metadata) -> u64 {
    use std::os::unix::fs::MetadataExt;

    metadata.blocks().saturating_mul(512)
}

#[cfg(not(unix))]
fn file_allocated_size(metadata: &fs::Metadata) -> u64 {
    metadata.len()
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
    use std::{collections::HashSet, fs};

    use tempfile::tempdir;

    use super::{
        directory_allocated_size, find_orphaned_disk_buffers, has_sufficient_buffer_capacity,
    };

    const GIBIBYTE: u64 = 1024 * 1024 * 1024;

    #[test]
    fn rejects_maximum_that_cannot_be_reached_with_current_free_space() {
        let maximum_buffer_bytes = 400 * GIBIBYTE;

        assert!(!has_sufficient_buffer_capacity(
            maximum_buffer_bytes,
            250 * GIBIBYTE,
            0,
        ));
        assert!(!has_sufficient_buffer_capacity(
            maximum_buffer_bytes,
            100 * GIBIBYTE,
            150 * GIBIBYTE,
        ));
        assert!(has_sufficient_buffer_capacity(
            maximum_buffer_bytes,
            250 * GIBIBYTE,
            150 * GIBIBYTE,
        ));

        let full_existing_buffer = 300 * GIBIBYTE;
        assert!(has_sufficient_buffer_capacity(
            full_existing_buffer,
            200 * GIBIBYTE,
            full_existing_buffer,
        ));
        assert!(!has_sufficient_buffer_capacity(
            full_existing_buffer,
            200 * GIBIBYTE,
            0,
        ));
    }

    #[test]
    fn finds_only_unconfigured_disk_buffer_directories() {
        let data_dir = tempdir().unwrap();
        let buffer_root = data_dir.path().join("buffer").join("v2");
        let namespace = buffer_root.join("namespace");
        let configured = namespace.join("configured");
        let orphaned = namespace.join("orphaned");
        fs::create_dir_all(&configured).unwrap();
        fs::create_dir(&orphaned).unwrap();
        fs::write(buffer_root.join("not-a-buffer"), b"ignored").unwrap();

        let configured_paths = HashSet::from([configured]);
        let actual = find_orphaned_disk_buffers(data_dir.path(), &configured_paths).unwrap();

        assert_eq!(actual, vec![orphaned]);
    }

    #[test]
    fn missing_disk_buffer_root_has_no_orphans() {
        let data_dir = tempdir().unwrap();

        let actual = find_orphaned_disk_buffers(data_dir.path(), &HashSet::new()).unwrap();

        assert!(actual.is_empty());
    }

    #[test]
    fn measures_files_in_nested_buffer_directories() {
        let data_dir = tempdir().unwrap();
        let nested = data_dir.path().join("nested");
        fs::create_dir(&nested).unwrap();
        fs::write(data_dir.path().join("ledger"), vec![0u8; 4096]).unwrap();
        fs::write(nested.join("buffer-data"), vec![0u8; 4096]).unwrap();

        assert!(directory_allocated_size(data_dir.path()).unwrap() >= 8192);
    }
}
