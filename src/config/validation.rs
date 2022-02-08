use std::collections::HashMap;

use vector_core::internal_event::DEFAULT_OUTPUT;

use super::{builder::ConfigBuilder, ComponentKey, Config, OutputId, Resource};

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
        let outputs = source.inner.outputs();
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
        let outputs = transform.inner.outputs();
        if outputs
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

pub fn warnings(config: &Config) -> Vec<String> {
    let mut warnings = vec![];

    let source_ids = config.sources.iter().flat_map(|(key, source)| {
        source
            .inner
            .outputs()
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
            .outputs()
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
