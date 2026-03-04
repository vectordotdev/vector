use super::schema::SchemaContext;
use anyhow::{Context, Result};
use serde_json::Value;
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

    // 1. Process Component Bases
    let mut component_bases = serde_json::Map::new();
    if let Some(definitions) = root_schema.get("definitions").and_then(|d| d.as_object()) {
        for (key, definition) in definitions {
            if let Some(base_type) =
                super::schema::get_schema_metadata(definition, "docs::component_base_type")
                    .and_then(|v| v.as_str())
                && component_types.contains(&base_type)
            {
                component_bases.insert(base_type.to_string(), Value::String(key.clone()));
            }
        }
    }

    for (comp_type, schema_name) in component_bases {
        render_and_import_generated_component_schema(
            &mut context,
            schema_name.as_str().unwrap(),
            &comp_type,
        )?;
    }

    // 2. Process All Components
    let mut all_components: std::collections::HashMap<String, serde_json::Map<String, Value>> =
        std::collections::HashMap::new();
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
                    .insert(n.to_string(), Value::String(key.clone()));
            }
        }
    }

    for (comp_type, components) in all_components {
        for (comp_name, schema_name) in components {
            render_and_import_component_schema(
                &mut context,
                schema_name.as_str().unwrap(),
                &comp_type,
                &comp_name,
            )?;
        }
    }

    // 3. Process APIs
    let mut apis = serde_json::Map::new();
    if let Some(definitions) = root_schema.get("definitions").and_then(|d| d.as_object()) {
        for (key, definition) in definitions {
            let comp_type = super::schema::get_schema_metadata(definition, "docs::component_type")
                .and_then(|v| v.as_str());
            let comp_name = super::schema::get_schema_metadata(definition, "docs::component_name")
                .and_then(|v| v.as_str());

            if comp_type == Some("api")
                && let Some(n) = comp_name
            {
                apis.insert(n.to_string(), Value::String(key.clone()));
            }
        }
    }
    render_and_import_generated_api_schema(&mut context, apis)?;

    // 4. Process Global Options
    let mut global_options = serde_json::Map::new();
    if let Some(definitions) = root_schema.get("definitions").and_then(|d| d.as_object()) {
        for (key, definition) in definitions {
            let comp_type = super::schema::get_schema_metadata(definition, "docs::component_type")
                .and_then(|v| v.as_str());
            let comp_name = super::schema::get_schema_metadata(definition, "docs::component_name")
                .and_then(|v| v.as_str());

            if comp_type == Some("global_option")
                && let Some(n) = comp_name
            {
                global_options.insert(n.to_string(), Value::String(key.clone()));
            }
        }
    }
    render_and_import_generated_global_option_schema(&mut context, global_options)?;

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
        error!(
            "[!]   Failed to import {} schema as valid Cue.",
            friendly_name
        );
        std::process::exit(1);
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

fn render_and_import_generated_api_schema(
    context: &mut SchemaContext,
    apis: serde_json::Map<String, Value>,
) -> Result<()> {
    let mut api_schema = serde_json::Map::new();
    for (component_name, schema_name) in apis {
        if let Some(name_str) = schema_name.as_str() {
            let friendly_name = format!("'{component_name}' api configuration");
            let resolved = context.unwrap_resolved_schema(name_str, &friendly_name)?;
            api_schema.insert(component_name, Value::Object(resolved));
        }
    }

    render_and_import_schema(
        context,
        Value::Object(api_schema),
        "configuration",
        &["generated", "api"],
        "generated/api.cue",
    )
}

fn render_and_import_generated_global_option_schema(
    context: &mut SchemaContext,
    global_options: serde_json::Map<String, Value>,
) -> Result<()> {
    let mut global_option_schema = serde_json::Map::new();

    for (component_name, schema_name) in global_options {
        if let Some(name_str) = schema_name.as_str() {
            let friendly_name = format!("'{component_name}' global options configuration");

            if component_name == "global_option" {
                let flattened = context.unwrap_resolved_schema(name_str, &friendly_name)?;
                for (k, v) in flattened {
                    global_option_schema.insert(k, v);
                }
            } else {
                let resolved = context.resolve_schema_by_name(name_str)?;
                global_option_schema.insert(component_name, resolved);
            }
        }
    }

    render_and_import_schema(
        context,
        Value::Object(global_option_schema),
        "configuration",
        &["generated", "configuration"],
        "generated/configuration.cue",
    )
}
