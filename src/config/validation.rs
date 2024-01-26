use crate::config::schema;
use futures_util::{stream, FutureExt, StreamExt, TryFutureExt, TryStreamExt};
use heim::{disk::Partition, units::information::byte};
use indexmap::IndexMap;
use std::{collections::HashMap, path::PathBuf};
use vector_lib::{buffers::config::DiskUsage, internal_event::DEFAULT_OUTPUT};

use super::{
    builder::ConfigBuilder, transform::get_transform_output_ids, ComponentKey, Config, OutputId,
    Resource,
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

pub fn warnings(config: &Config) -> Vec<String> {
    let mut warnings = vec![];

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
        get_transform_output_ids(
            transform.inner.as_ref(),
            key.clone(),
            config.schema.log_namespace(),
        )
        .map(|output| ("transform", output))
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
