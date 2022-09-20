use crate::config::schema;
use crate::topology::schema::merged_definition;
use futures_util::StreamExt;
use indexmap::IndexMap;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};
use sysinfo::{DiskExt, System, SystemExt};
use vector_core::internal_event::DEFAULT_OUTPUT;

use super::{
    builder::ConfigBuilder, ComponentKey, Config, OutputId, Resource, SourceConfig, TransformConfig,
};

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

    if config.sources.is_empty() {
        errors.push("No sources defined in the config.".to_owned());
    }

    if config.sinks.is_empty() {
        errors.push("No sinks defined in the config.".to_owned());
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
            let entry = frequencies.entry(input.clone()).or_insert(0usize);
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
                format!(
                    "Resource `{}` is claimed by multiple components: {:?}",
                    resource, components
                )
            })
            .collect())
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
        // use the most general definition possible, since the real value isn't known yet.
        let definition = schema::Definition::any();

        if let Err(errs) = transform.inner.validate(&definition) {
            errors.extend(errs.into_iter().map(|msg| format!("Transform {key} {msg}")));
        }

        if transform
            .inner
            .outputs(&definition)
            .iter()
            .map(|output| output.port.as_deref().unwrap_or(""))
            .any(|name| name == DEFAULT_OUTPUT)
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

    // Query all the disks on the system, sorted by their mountpoint -- longest to shortest --
    // and create a map of mountpoint => total capacity.
    //
    // This is so that we can find the most specific mountpoint for a given disk buffer just by
    // iterating in normal beginning-to-end fashion until we find a mountpoint path that is a prefix
    // of the buffer's data directory path.
    let mut system_info = System::new();
    system_info.refresh_disks_list();
    system_info.sort_disks_by(|a, b| b.mount_point().cmp(a.mount_point()));
    let mountpoints = system_info
        .disks()
        .iter()
        .map(|disk| (disk.mount_point().to_path_buf(), disk.total_space()))
        .collect::<IndexMap<_, _>>();

    let heim_mountpoints = heim::disk::partitions().await
        .expect("getting partitions should not fail")
        .filter_map(|result| async move {
            match result {
                Ok(partition) => Some(partition),
                Err(e) => {
                    println!("partition error: {}", e);
                    None
                },
            }
        })
        .collect::<Vec<_>>()
        .await;
    println!("heim mountpoints: {:#?}", heim_mountpoints);

    // Now build a mapping of buffer IDs/usage configuration to the mountpoint they reside on.
    let global_data_dir = config.global.data_dir.clone();
    let mountpoint_buffer_mapping = config
        .sinks()
        .flat_map(|(id, sink)| {
            sink.buffer
                .stages()
                .iter()
                .filter_map(|stage| stage.disk_usage(global_data_dir.clone(), id))
        })
        .fold(HashMap::new(), |mut mappings, usage| {
            let resolved_data_dir = get_resolved_dir_path(usage.data_dir());
            let mountpoint = mountpoints
                .keys()
                .find(|mountpoint| resolved_data_dir.starts_with(mountpoint));

            match mountpoint {
                // TODO: Should this actually be an error?
                //
                // My inclination is that because we're trying to stop operators from landing in bad
                // outcomes, like running out of space and panicking/corrupting some of the buffer
                // data, we would want to be authoritative: if we can't be sure the mountpoints are
                // large enough, we don't validate the configuration.
                //
                // On the other hand, we need to be sure this code works correctly on all of our
                // "tier 1" platforms -- i.e. Linux, Windows, macOS, and in containerized
                // environments -- before we could feel confident enough, I think, to block Vector
                // startup on such a validation.
                //
                // If we can indeed show that this code works correctly on those environments, such
                // that we can be reasonably confident that normal users won't encounter errors that
                // hamstring them, then maybe we could make this an actual failure instead of just a
                // warning log.
                None => warn!(
                    buffer_id = usage.id().id(),
                    buffer_data_dir = usage.data_dir().to_string_lossy().as_ref(),
                    resolved_buffer_data_dir = resolved_data_dir.to_string_lossy().as_ref(),
                    message = "Found no matching mountpoint for buffer data directory.",
                ),
                Some(mountpoint) => {
                    let entries = mappings.entry(mountpoint.clone()).or_insert_with(Vec::new);

                    entries.push(usage);
                }
            }

            mappings
        });

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

#[cfg(target_os = "macos")]
fn get_resolved_dir_path(path: &Path) -> PathBuf {
    // On more recent versions of macOS, they use a layered approach to the filesystem where the
    // common "root" filesystem -- i.e. `/` -- is actually composed of various directories which are
    // sourced from various underlying volumes, where some directories are read only (like OS base
    // files) and some are read/write (like user home directories and `/tmp/` and so on).
    //
    // For the common paths like user folders, or `/tmp`, and so on, we can't actually directly
    // resolve the volume that they're magically mapped to unless we specifically use information
    // from the "firmlinks" mapping.

    // We try to load the firmlinks mapping file, and if we can, we read it, and read the
    // associations within it.
    let firmlinks_data = std::fs::read_to_string("/usr/share/firmlinks");
    match firmlinks_data {
        // If we got an error trying to read the firmlinks mapping file, just return the original path.
        Err(_) => path.to_path_buf(),
        Ok(data) => {
            let data_volume = PathBuf::from("/System/Volumes/Data");

            // Parse the mapping data, which contains multiple lines with a tab-delimited
            // key/value pair, the key being the path to map and the value being the relative
            // directory within the data volume to map to.
            let firmlink_mappings = data
                .split('\n')
                .filter_map(|line| {
                    let parts = line.split('\t').collect::<Vec<_>>();
                    if parts.len() != 2 {
                        None
                    } else {
                        let overlay_destination_path = PathBuf::from(parts[0]);
                        let overlay_source_path = data_volume.clone().join(parts[1]);

                        // Make sure both paths exist on disk as a safeguard against the format
                        // changing, or our understanding of how to utilize the file being
                        // inaccurate, etc.
                        if Path::exists(&overlay_destination_path)
                            && Path::exists(&overlay_source_path)
                        {
                            Some((overlay_destination_path, overlay_source_path))
                        } else {
                            None
                        }
                    }
                })
                .collect::<HashMap<_, _>>();

            // Now that we have the firmlinks mappings read and parsed, iterate through them to
            // see if any of them is a prefix of the path we've been given, and if so, we'll
            // replace that prefix with the mapping version.
            match firmlink_mappings
                .iter()
                .find(|(prefix, _)| path.starts_with(prefix))
            {
                Some((prefix, replacement)) => {
                    // Strip the prefix, and then join the stripped version to our data volume
                    // path along with the prefix replacement.
                    let stripped = path
                        .strip_prefix(&prefix)
                        .expect("path is known to be prefixed by value");
                    replacement.clone().join(stripped)
                }
                // There was no firmlink mapping related to the path being resolved, so return
                // it as-is.
                None => path.to_path_buf(),
            }
        }
    }
}

#[cfg(not(target_os = "macos"))]
fn get_resolved_dir_path(path: &Path) -> PathBuf {
    path.to_path_buf()
}

pub fn warnings(config: &Config) -> Vec<String> {
    let mut warnings = vec![];
    let mut cache = HashMap::new();

    let source_ids = config.sources.iter().flat_map(|(key, source)| {
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
        transform
            .inner
            .outputs(&merged_definition(&transform.inputs, config, &mut cache))
            .iter()
            .map(|output| {
                if let Some(port) = &output.port {
                    ("transform", OutputId::from((key, port.clone())))
                } else {
                    ("transform", OutputId::from(key))
                }
            })
            .collect::<Vec<_>>()
    });

    for (input_type, id) in transform_ids.chain(source_ids) {
        if !config
            .transforms
            .iter()
            .any(|(_, transform)| transform.inputs.contains(&id))
            && !config
                .sinks
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
