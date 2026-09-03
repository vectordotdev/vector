use super::schema::SchemaContext;
use anyhow::{Context, Result, bail};
use indexmap::IndexMap;
use serde_json::{Value, json};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const CUE_DEFINITIONS_FIELD: &str = "#SchemaDefinitions";
const CUE_DEFINITIONS_PLACEHOLDER_FIELD: &str = "vector_internal_schema_definitions";
const CUE_REFERENCE_MARKER_PREFIX: &str = "__VECTOR_CUE_REFERENCE__";

// Component schemas are resolved independently before being imported from JSON into CUE. Types
// marked with `docs::cue_definition` are interned here so repeated resolved values can instead
// point at one CUE definition. The marker is necessary because JSON cannot represent a CUE
// reference; it is replaced with a CUE expression immediately after `cue import`.
struct CueDefinitions {
    values: IndexMap<String, Value>,
    usage_counts: IndexMap<String, usize>,
}

impl CueDefinitions {
    fn from_schema(context: &mut SchemaContext) -> Result<Self> {
        let definitions = context
            .root_schema
            .get("definitions")
            .and_then(Value::as_object)
            .into_iter()
            .flatten();
        let mut named_schemas = Vec::new();
        for (schema_name, definition) in definitions {
            if let Some(cue_name) =
                super::schema::get_schema_metadata(definition, "docs::cue_definition")
            {
                let cue_name = cue_name.as_str().ok_or_else(|| {
                    anyhow::anyhow!(
                        "CUE definition name for schema '{schema_name}' must be a string"
                    )
                })?;
                named_schemas.push((cue_name.to_string(), schema_name.clone()));
            }
        }
        named_schemas.sort();

        let mut values = IndexMap::new();
        for (cue_name, schema_name) in named_schemas {
            let resolved = context.resolve_schema_by_name(&schema_name)?;
            let resolved_type = resolved.get("type").and_then(Value::as_object).ok_or_else(
                || {
                    anyhow::anyhow!(
                        "CUE definition schema '{schema_name}' did not resolve to a documented type"
                    )
                },
            )?;
            let resolved_type = Value::Object(SchemaContext::sort_hash_nested(resolved_type));
            if values.insert(cue_name.clone(), resolved_type).is_some() {
                bail!("Duplicate CUE definition name '{cue_name}'");
            }
        }

        let usage_counts = values.keys().map(|name| (name.clone(), 0)).collect();
        Ok(Self {
            values,
            usage_counts,
        })
    }

    fn replace_references(&mut self, value: &mut Value) {
        for (cue_name, definition) in &self.values {
            if value == definition {
                *value = Value::String(format!("{CUE_REFERENCE_MARKER_PREFIX}{cue_name}"));
                *self
                    .usage_counts
                    .get_mut(cue_name)
                    .expect("usage count exists for every CUE definition") += 1;
                return;
            }
        }

        match value {
            Value::Array(values) => {
                for value in values {
                    self.replace_references(value);
                }
            }
            Value::Object(values) => {
                for value in values.values_mut() {
                    self.replace_references(value);
                }
            }
            _ => {}
        }
    }

    fn rewrite_reference_markers(&self, content: &mut String) -> Result<()> {
        for cue_name in self.values.keys() {
            let marker =
                serde_json::to_string(&format!("{CUE_REFERENCE_MARKER_PREFIX}{cue_name}"))?;
            let cue_name = serde_json::to_string(cue_name)?;
            *content = content.replace(&marker, &format!("{CUE_DEFINITIONS_FIELD}[{cue_name}]"));
        }
        if content.contains(CUE_REFERENCE_MARKER_PREFIX) {
            bail!("Generated CUE contained an unresolved reference marker");
        }
        Ok(())
    }

    fn ensure_all_reused(&self) -> Result<()> {
        let not_reused = self
            .usage_counts
            .iter()
            .filter_map(|(name, count)| (*count < 2).then_some(name.as_str()))
            .collect::<Vec<_>>();
        if !not_reused.is_empty() {
            bail!(
                "CUE definitions must each replace at least two generated values: {}",
                not_reused.join(", ")
            );
        }
        Ok(())
    }
}

pub fn run(schema_path: &Path) -> Result<()> {
    let schema_content = fs::read_to_string(schema_path)
        .with_context(|| format!("Failed to read schema file from {}", schema_path.display()))?;

    let root_schema: Value =
        serde_json::from_str(&schema_content).with_context(|| "Failed to parse schema JSON")?;

    let mut context = SchemaContext::new(root_schema.clone())?;
    let mut cue_definitions = CueDefinitions::from_schema(&mut context)?;

    let component_types = ["source", "transform", "sink"];

    // 1. Process Component Bases (sorted by component type for deterministic output)
    let mut component_bases: IndexMap<String, String> = IndexMap::new();
    if let Some(definitions) = root_schema.get("definitions").and_then(|d| d.as_object()) {
        for (key, definition) in definitions {
            if let Some(base_type) =
                super::schema::get_schema_metadata(definition, "docs::component_base_type")
                    .and_then(|v| v.as_str())
                && component_types.contains(&base_type)
            {
                component_bases.insert(base_type.to_string(), key.clone());
            }
        }
    }
    component_bases.sort_keys();

    for (comp_type, schema_name) in &component_bases {
        render_and_import_generated_component_schema(
            &mut context,
            &mut cue_definitions,
            schema_name,
            comp_type,
        )?;
    }

    // 2. Process All Components (sorted by component type and name for deterministic output)
    let mut all_components: IndexMap<String, IndexMap<String, String>> = IndexMap::new();
    if let Some(definitions) = root_schema.get("definitions").and_then(|d| d.as_object()) {
        for (key, definition) in definitions {
            let comp_type = super::schema::get_schema_metadata(definition, "docs::component_type")
                .and_then(|v| v.as_str());
            let comp_name = super::schema::get_schema_metadata(definition, "docs::component_name")
                .and_then(|v| v.as_str());

            if let (Some(t), Some(n)) = (comp_type, comp_name)
                && component_types.contains(&t)
            {
                all_components
                    .entry(t.to_string())
                    .or_default()
                    .insert(n.to_string(), key.clone());
            }
        }
    }
    all_components.sort_keys();
    for (_, components) in &mut all_components {
        components.sort_keys();
    }

    for (comp_type, components) in &all_components {
        for (comp_name, schema_name) in components {
            render_and_import_component_schema(
                &mut context,
                &mut cue_definitions,
                schema_name,
                comp_type,
                comp_name,
            )?;
        }
    }

    // 3. Process top-level configuration fields (formerly "global options").
    // The standalone API schema (`generated/api.cue`) was retired in #24858; api
    // is now rendered as a top-level field with `group: "api"`.
    render_and_import_generated_top_level_config_schema(
        &mut context,
        &mut cue_definitions,
        &root_schema,
    )?;
    cue_definitions.ensure_all_reused()?;
    render_and_import_cue_definitions(&context, &cue_definitions)?;

    Ok(())
}

fn write_to_temp_file(prefix: &str, suffix: &str, content: &str) -> Result<PathBuf> {
    use std::io::Write;
    let mut tmp = tempfile::Builder::new()
        .prefix(prefix)
        .suffix(suffix)
        .tempfile()?;
    tmp.write_all(content.as_bytes())?;
    let path = tmp.into_temp_path().keep()?;
    Ok(path)
}

fn import_json_as_cue(
    context: &SchemaContext,
    cue_definitions: Option<&CueDefinitions>,
    data: &Value,
    friendly_name: &str,
    prefix: &str,
    cue_output_file: &Path,
) -> Result<()> {
    let final_json = serde_json::to_string_pretty(data)?;
    let json_output_file = write_to_temp_file(prefix, ".json", &final_json)?;

    debug!(
        "[✓]   Wrote {} schema to '{}'. ({} bytes)",
        friendly_name,
        json_output_file.display(),
        final_json.len()
    );
    debug!("[*] Importing {} schema as Cue file...", friendly_name);

    if let Some(parent) = cue_output_file.parent() {
        fs::create_dir_all(parent)?;
    }
    let status = Command::new(&context.cue_binary_path)
        .args([
            "import",
            "-f",
            "-o",
            cue_output_file.to_str().unwrap(),
            "-p",
            "metadata",
            json_output_file.to_str().unwrap(),
        ])
        .status()?;
    if !status.success() {
        bail!(
            "Failed to import {friendly_name} schema as valid Cue (cue exit status {status}). JSON written to {json_path}.",
            json_path = json_output_file.display()
        );
    }

    if let Some(cue_definitions) = cue_definitions {
        let mut cue_output = fs::read_to_string(cue_output_file)?;
        cue_definitions.rewrite_reference_markers(&mut cue_output)?;
        fs::write(cue_output_file, cue_output)?;
    }

    debug!(
        "[✓]   Imported {} schema to '{}'.",
        friendly_name,
        cue_output_file.display()
    );
    Ok(())
}

fn render_and_import_schema(
    context: &mut SchemaContext,
    cue_definitions: &mut CueDefinitions,
    unwrapped_resolved_schema: Value,
    friendly_name: &str,
    config_map_path: &[&str],
    cue_relative_path: &str,
) -> Result<()> {
    let mut data = serde_json::Map::new();
    // Simplified nesting since serde doesn't make building deeply nested objects inline easy
    // In practice, this needs to build a nested path of objects and put `configuration` at the end

    let mut current_obj = &mut data;
    for segment in config_map_path {
        current_obj.insert(
            (*segment).to_string(),
            Value::Object(serde_json::Map::new()),
        );
        current_obj = current_obj
            .get_mut(*segment)
            .unwrap()
            .as_object_mut()
            .unwrap();
    }
    current_obj.insert("configuration".to_string(), unwrapped_resolved_schema);

    let mut data = Value::Object(data);
    cue_definitions.replace_references(&mut data);

    let prefix = format!("config-schema-base-{}-", config_map_path.join("-"));
    let cue_output_file = PathBuf::from("website/cue/reference").join(cue_relative_path);
    import_json_as_cue(
        context,
        Some(cue_definitions),
        &data,
        friendly_name,
        &prefix,
        &cue_output_file,
    )
}

fn render_and_import_generated_component_schema(
    context: &mut SchemaContext,
    cue_definitions: &mut CueDefinitions,
    schema_name: &str,
    component_type: &str,
) -> Result<()> {
    let friendly_name = format!("generated {component_type} configuration");
    let unwrapped = context.unwrap_resolved_schema(schema_name, &friendly_name)?;
    let cue_path = format!("components/generated/{component_type}s.cue");

    render_and_import_schema(
        context,
        cue_definitions,
        Value::Object(unwrapped),
        &friendly_name,
        &["generated", "components", &format!("{component_type}s")],
        &cue_path,
    )
}

fn render_and_import_component_schema(
    context: &mut SchemaContext,
    cue_definitions: &mut CueDefinitions,
    schema_name: &str,
    component_type: &str,
    component_name: &str,
) -> Result<()> {
    let friendly_name = format!("'{component_name}' {component_type} configuration");
    let unwrapped = context.unwrap_resolved_schema(schema_name, &friendly_name)?;
    let cue_path = format!("components/{component_type}s/generated/{component_name}.cue");

    render_and_import_schema(
        context,
        cue_definitions,
        Value::Object(unwrapped),
        &friendly_name,
        &[
            "generated",
            "components",
            &format!("{component_type}s"),
            component_name,
        ],
        &cue_path,
    )
}

// Field-to-group mapping. Fields not listed default to "global_options".
const TOP_LEVEL_FIELD_GROUPS: &[(&str, &str)] = &[
    ("sources", "pipeline_components"),
    ("transforms", "pipeline_components"),
    ("sinks", "pipeline_components"),
    ("enrichment_tables", "pipeline_components"),
    ("api", "api"),
    ("schema", "schema"),
    ("log_schema", "schema"),
    ("secret", "secrets"),
];

fn top_level_group_metadata() -> Value {
    json!({
        "global_options": {
            "title": "Global Options",
            "description": "Global configuration options that apply to Vector as a whole.",
            "order": 1,
        },
        "pipeline_components": {
            "title": "Pipeline Components",
            "description": "Configure sources, transforms, sinks, and enrichment tables for your observability pipeline.",
            "order": 2,
        },
        "api": {
            "title": "API",
            "description": "Configure Vector's observability API.",
            "order": 3,
        },
        "schema": {
            "title": "Schema",
            "description": "Configure Vector's internal schema system for type tracking and validation.",
            "order": 4,
        },
        "secrets": {
            "title": "Secrets",
            "description": "Configure secrets management for secure configuration.",
            "order": 5,
        },
    })
}

fn resolve_top_level_config_fields(
    context: &mut SchemaContext,
    root_schema: &Value,
) -> Result<serde_json::Map<String, Value>> {
    // ConfigBuilder uses #[serde(flatten)] for GlobalOptions, so root_schema.allOf
    // contains multiple subschemas whose properties together form the top-level config.
    let all_of = root_schema
        .get("allOf")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow::anyhow!("Could not find ConfigBuilder allOf schemas"))?;
    if all_of.is_empty() {
        anyhow::bail!("ConfigBuilder allOf schemas are empty");
    }

    let mut all_properties: IndexMap<String, Value> = IndexMap::new();
    for subschema in all_of {
        if let Some(props) = subschema.get("properties").and_then(Value::as_object) {
            for (k, v) in props {
                all_properties.insert(k.clone(), v.clone());
            }
        }
    }

    let mut resolved_fields = serde_json::Map::new();
    for (field_name, field_schema) in all_properties {
        if super::schema::get_schema_metadata(&field_schema, "docs::hidden").is_some() {
            debug!("Skipping '{field_name}' (marked as docs::hidden)");
            continue;
        }

        let mut resolved = context.resolve_schema(&field_schema)?;
        if !resolved.is_object() {
            continue;
        }

        let group = TOP_LEVEL_FIELD_GROUPS
            .iter()
            .find(|(name, _)| *name == field_name)
            .map_or("global_options", |(_, g)| *g);

        resolved
            .as_object_mut()
            .unwrap()
            .insert("group".to_string(), Value::String(group.to_string()));

        resolved_fields.insert(field_name, resolved);
    }
    Ok(resolved_fields)
}

fn render_and_import_generated_top_level_config_schema(
    context: &mut SchemaContext,
    cue_definitions: &mut CueDefinitions,
    root_schema: &Value,
) -> Result<()> {
    let resolved_fields = resolve_top_level_config_fields(context, root_schema)?;

    let mut data = json!({
        "generated": {
            "configuration": {
                "configuration": Value::Object(resolved_fields),
                "groups": top_level_group_metadata(),
            }
        }
    });
    cue_definitions.replace_references(&mut data);

    let cue_output_file =
        PathBuf::from("website/cue/reference").join("generated/configuration.cue");
    import_json_as_cue(
        context,
        Some(cue_definitions),
        &data,
        "configuration",
        "config-schema-base-generated-configuration-",
        &cue_output_file,
    )
}

fn render_and_import_cue_definitions(
    context: &SchemaContext,
    cue_definitions: &CueDefinitions,
) -> Result<()> {
    let data = json!({
        (CUE_DEFINITIONS_PLACEHOLDER_FIELD): cue_definitions.values,
    });
    let cue_output_file =
        PathBuf::from("website/cue/reference/components/generated/schema_definitions.cue");
    import_json_as_cue(
        context,
        None,
        &data,
        "shared schema definitions",
        "config-schema-cue-definitions-",
        &cue_output_file,
    )?;

    let cue_output = fs::read_to_string(&cue_output_file)?;
    let cue_output = mark_cue_definitions_field(&cue_output)?;
    fs::write(&cue_output_file, cue_output)?;

    Ok(())
}

fn mark_cue_definitions_field(cue_output: &str) -> Result<String> {
    let placeholder = format!("{CUE_DEFINITIONS_PLACEHOLDER_FIELD}:");
    if cue_output.matches(&placeholder).count() != 1 {
        bail!("Generated CUE definitions must contain exactly one placeholder field");
    }
    Ok(cue_output.replacen(&placeholder, &format!("{CUE_DEFINITIONS_FIELD}:"), 1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replaces_repeated_values_with_cue_references() {
        let cue_name = "shared_options".to_string();
        let definition = json!({"object": {"options": {"limit": {"type": {"uint": {}}}}}});
        let mut values = IndexMap::new();
        values.insert(cue_name.clone(), definition.clone());
        let mut usage_counts = IndexMap::new();
        usage_counts.insert(cue_name, 0);
        let mut definitions = CueDefinitions {
            values,
            usage_counts,
        };
        let mut generated = json!({
            "first": {"type": definition.clone()},
            "second": {"type": definition},
        });

        definitions.replace_references(&mut generated);
        definitions.ensure_all_reused().unwrap();

        let marker = Value::String(format!("{CUE_REFERENCE_MARKER_PREFIX}shared_options"));
        assert_eq!(generated.pointer("/first/type"), Some(&marker));
        assert_eq!(generated.pointer("/second/type"), Some(&marker));
    }

    #[test]
    fn rewrites_imported_markers_as_cue_expressions() {
        let mut values = IndexMap::new();
        values.insert("shared_options".to_string(), json!({}));
        let definitions = CueDefinitions {
            values,
            usage_counts: IndexMap::new(),
        };
        let mut cue = format!("type: \"{CUE_REFERENCE_MARKER_PREFIX}shared_options\"\n");

        definitions.rewrite_reference_markers(&mut cue).unwrap();

        assert_eq!(cue, "type: #SchemaDefinitions[\"shared_options\"]\n");
    }

    #[test]
    fn marks_imported_definition_storage_as_a_definition() {
        let cue =
            format!("package metadata\n\n{CUE_DEFINITIONS_PLACEHOLDER_FIELD}: shared: {{}}\n");

        assert_eq!(
            mark_cue_definitions_field(&cue).unwrap(),
            "package metadata\n\n#SchemaDefinitions: shared: {}\n"
        );
    }
}
