use super::schema::SchemaContext;
use anyhow::{Context, Result, bail};
use indexmap::IndexMap;
use serde_json::{Value, json};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

pub fn run(schema_path: &Path) -> Result<()> {
    let schema_content = fs::read_to_string(schema_path)
        .with_context(|| format!("Failed to read schema file from {}", schema_path.display()))?;

    let root_schema: Value =
        serde_json::from_str(&schema_content).with_context(|| "Failed to parse schema JSON")?;

    let mut context = SchemaContext::new(root_schema.clone())?;

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
        render_and_import_generated_component_schema(&mut context, schema_name, comp_type)?;
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
            render_and_import_component_schema(&mut context, schema_name, comp_type, comp_name)?;
        }
    }

    // 3. Process top-level configuration fields (formerly "global options").
    // The standalone API schema (`generated/api.cue`) was retired in #24858; api
    // is now rendered as a top-level field with `group: "api"`.
    render_and_import_generated_top_level_config_schema(&mut context, &root_schema)?;

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

fn render_and_import_schema(
    context: &mut SchemaContext,
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

    let mut prefix = String::from("config-schema-base-");
    prefix.push_str(&config_map_path.join("-"));
    prefix.push('-');

    let final_json = serde_json::to_string_pretty(&data)?;
    let json_output_file = write_to_temp_file(&prefix, ".json", &final_json)?;

    info!(
        "[✓]   Wrote {} schema to '{}'. ({} bytes)",
        friendly_name,
        json_output_file.display(),
        final_json.len()
    );

    info!("[*] Importing {} schema as Cue file...", friendly_name);
    let cue_output_file = PathBuf::from("website/cue/reference").join(cue_relative_path);

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

    info!(
        "[✓]   Imported {} schema to '{}'.",
        friendly_name,
        cue_output_file.display()
    );
    Ok(())
}

fn render_and_import_generated_component_schema(
    context: &mut SchemaContext,
    schema_name: &str,
    component_type: &str,
) -> Result<()> {
    let friendly_name = format!("generated {component_type} configuration");
    let unwrapped = context.unwrap_resolved_schema(schema_name, &friendly_name)?;
    let cue_path = format!("components/generated/{component_type}s.cue");

    render_and_import_schema(
        context,
        Value::Object(unwrapped),
        &friendly_name,
        &["generated", "components", &format!("{component_type}s")],
        &cue_path,
    )
}

fn render_and_import_component_schema(
    context: &mut SchemaContext,
    schema_name: &str,
    component_type: &str,
    component_name: &str,
) -> Result<()> {
    let friendly_name = format!("'{component_name}' {component_type} configuration");
    let unwrapped = context.unwrap_resolved_schema(schema_name, &friendly_name)?;
    let cue_path = format!("components/{component_type}s/generated/{component_name}.cue");

    render_and_import_schema(
        context,
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
    root_schema: &Value,
) -> Result<()> {
    let resolved_fields = resolve_top_level_config_fields(context, root_schema)?;

    let data = json!({
        "generated": {
            "configuration": {
                "configuration": Value::Object(resolved_fields),
                "groups": top_level_group_metadata(),
            }
        }
    });

    let friendly_name = "configuration";
    let prefix = "config-schema-base-generated-configuration-";

    let final_json = serde_json::to_string_pretty(&data)?;
    let json_output_file = write_to_temp_file(prefix, ".json", &final_json)?;

    info!(
        "[✓]   Wrote {} schema to '{}'. ({} bytes)",
        friendly_name,
        json_output_file.display(),
        final_json.len()
    );

    info!("[*] Importing {} schema as Cue file...", friendly_name);
    let cue_output_file =
        PathBuf::from("website/cue/reference").join("generated/configuration.cue");

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

    info!(
        "[✓]   Imported {} schema to '{}'.",
        friendly_name,
        cue_output_file.display()
    );
    Ok(())
}
