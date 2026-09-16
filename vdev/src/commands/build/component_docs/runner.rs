use super::schema::SchemaContext;
use anyhow::{Context, Result, bail};
use indexmap::IndexMap;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet, hash_map::Entry};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const CUE_DEFINITIONS_FIELD: &str = "_schemaDefinitions";
const CUE_DEFINITIONS_PLACEHOLDER_FIELD: &str = "vector_internal_schema_definitions";
const CUE_REFERENCE_MARKER_PREFIX: &str = "__VECTOR_CUE_REFERENCE__";
const CUE_DEFINITIONS_DIRECTORY: &str =
    "website/cue/reference/components/generated/schema_definitions";

// Component schemas are resolved independently before being imported from JSON into CUE. Reused
// JSON Schema definitions are interned here so repeated resolved values can instead point at one
// shared CUE value. The marker is necessary because JSON cannot represent a CUE reference; it is
// replaced with a CUE expression immediately after `cue import`.
struct CueDefinitions {
    values: IndexMap<String, Value>,
    usage_counts: IndexMap<String, usize>,
    names_by_value: HashMap<String, String>,
}

impl CueDefinitions {
    fn from_schema(context: &mut SchemaContext) -> Result<Self> {
        let mut schema_names = context
            .root_schema
            .get("definitions")
            .and_then(Value::as_object)
            .into_iter()
            .flatten()
            .map(|(name, _)| name.clone())
            .collect::<Vec<_>>();
        schema_names.sort();

        let mut values = IndexMap::new();
        let mut names_by_value = HashMap::new();
        for schema_name in schema_names {
            let resolved = match context.resolve_schema_by_name(&schema_name) {
                Ok(resolved) => resolved,
                Err(error) => {
                    debug!("Skipping schema definition '{schema_name}': {error:#}");
                    continue;
                }
            };
            let Some(resolved_type) = resolved.get("type").and_then(Value::as_object) else {
                continue;
            };
            if resolved_type.get("object").is_none() {
                continue;
            }
            let resolved_type = Value::Object(SchemaContext::sort_hash_nested(resolved_type));
            let serialized = canonical_value(&resolved_type);
            if let Entry::Vacant(entry) = names_by_value.entry(serialized) {
                entry.insert(schema_name.clone());
                values.insert(schema_name, resolved_type);
            }
        }

        let usage_counts = values.keys().map(|name| (name.clone(), 0)).collect();
        Ok(Self {
            values,
            usage_counts,
            names_by_value,
        })
    }

    fn count_references(&mut self, value: &Value) {
        if let Some(cue_name) = self.definition_name(value) {
            *self
                .usage_counts
                .get_mut(&cue_name)
                .expect("usage count exists for every CUE definition") += 1;
        }

        match value {
            Value::Array(values) => {
                for value in values {
                    self.count_references(value);
                }
            }
            Value::Object(values) => {
                for value in values.values() {
                    self.count_references(value);
                }
            }
            _ => {}
        }
    }

    fn discover_reused_anonymous_values<'a>(&mut self, roots: impl IntoIterator<Item = &'a Value>) {
        let mut named_definitions_by_shape = HashMap::<String, Vec<String>>::new();
        for (name, value) in &self.values {
            named_definitions_by_shape
                .entry(canonical_value(&schema_shape(value)))
                .or_default()
                .push(name.clone());
        }

        let mut candidates = HashMap::<String, (Value, usize)>::new();
        for root in roots {
            collect_object_values(root, &mut candidates);
        }

        let mut reusable = candidates
            .into_iter()
            .filter(|(serialized, (_, count))| {
                *count >= 2 && !self.names_by_value.contains_key(serialized)
            })
            .map(|(serialized, (value, _))| {
                let shape = canonical_value(&schema_shape(&value));
                let hint = named_definitions_by_shape
                    .get(&shape)
                    .filter(|names| names.len() == 1)
                    .map(|names| names[0].as_str());
                let name = derived_definition_name(&value, hint);
                (name, serialized, value)
            })
            .collect::<Vec<_>>();
        reusable.sort_by(|left, right| left.0.cmp(&right.0).then(left.1.cmp(&right.1)));

        let mut name_counts = HashMap::<String, usize>::new();
        for (name, _, _) in &reusable {
            *name_counts.entry(name.clone()).or_default() += 1;
        }

        for (name, serialized, value) in reusable {
            // Different schemas can have the same documentation-independent identity. Avoid
            // giving either an unstable content-derived suffix; named definitions still cover
            // those schemas when available.
            if name_counts.get(&name) != Some(&1) {
                continue;
            }

            self.names_by_value.insert(serialized, name.clone());
            self.usage_counts.insert(name.clone(), 0);
            self.values.insert(name, value);
        }

        self.values.sort_keys();
        self.usage_counts.sort_keys();
    }

    fn retain_reused(&mut self) {
        self.retain_with_minimum_usage(2);
    }

    fn retain_used(&mut self) {
        self.retain_with_minimum_usage(1);
    }

    fn retain_with_minimum_usage(&mut self, minimum: usize) {
        self.values.retain(|name, _| {
            self.usage_counts
                .get(name)
                .is_some_and(|count| *count >= minimum)
        });
        self.usage_counts
            .retain(|name, _| self.values.contains_key(name));
        self.names_by_value
            .retain(|_, name| self.values.contains_key(name));
    }

    fn reset_usage_counts(&mut self) {
        for count in self.usage_counts.values_mut() {
            *count = 0;
        }
    }

    fn replace_references(&mut self, value: &mut Value) {
        self.replace_references_except(value, None);
    }

    fn replace_references_in_used_definitions(&mut self) {
        let mut processed = HashSet::new();
        loop {
            let referenced = self
                .usage_counts
                .iter()
                .filter(|(name, count)| **count > 0 && !processed.contains(*name))
                .map(|(name, _)| name.clone())
                .collect::<Vec<_>>();
            if referenced.is_empty() {
                break;
            }

            for name in referenced {
                let mut value = self
                    .values
                    .shift_remove(&name)
                    .expect("referenced CUE definition has a value");
                if let Value::Object(root) = &mut value {
                    for child in root.values_mut() {
                        self.replace_references_except(child, Some(&name));
                    }
                }
                self.values.insert(name.clone(), value);
                processed.insert(name);
            }
        }
        self.values.sort_keys();
    }

    fn replace_references_except(&mut self, value: &mut Value, excluded_name: Option<&str>) {
        if let Some(cue_name) = self.definition_name(value)
            && excluded_name != Some(&cue_name)
        {
            *value = Value::String(format!("{CUE_REFERENCE_MARKER_PREFIX}{cue_name}"));
            *self
                .usage_counts
                .get_mut(&cue_name)
                .expect("usage count exists for every CUE definition") += 1;
            return;
        }

        match value {
            Value::Array(values) => {
                for value in values {
                    self.replace_references_except(value, excluded_name);
                }
            }
            Value::Object(values) => {
                for value in values.values_mut() {
                    self.replace_references_except(value, excluded_name);
                }
            }
            _ => {}
        }
    }

    fn definition_name(&self, value: &Value) -> Option<String> {
        value.get("object")?;
        self.names_by_value.get(&canonical_value(value)).cloned()
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
}

fn canonical_value(value: &Value) -> String {
    let object = value
        .as_object()
        .expect("only resolved object types are canonicalized");
    serde_json::to_string(&Value::Object(SchemaContext::sort_hash_nested(object)))
        .expect("serializing a JSON value cannot fail")
}

fn collect_object_values(value: &Value, candidates: &mut HashMap<String, (Value, usize)>) {
    if value.get("object").is_some() {
        let serialized = canonical_value(value);
        let (_, count) = candidates
            .entry(serialized)
            .or_insert_with(|| (value.clone(), 0));
        *count += 1;
    }

    match value {
        Value::Array(values) => {
            for value in values {
                collect_object_values(value, candidates);
            }
        }
        Value::Object(values) => {
            for value in values.values() {
                collect_object_values(value, candidates);
            }
        }
        _ => {}
    }
}

fn derived_definition_name(value: &Value, hint: Option<&str>) -> String {
    let identity = schema_identity(value);
    let serialized = canonical_value(&identity);
    let digest = Sha256::digest(serialized.as_bytes());
    let digest = hex::encode(&digest[..12]);
    hint.map_or_else(
        || format!("derived::{digest}"),
        |hint| format!("derived::{hint}::{digest}"),
    )
}

fn schema_shape(value: &Value) -> Value {
    match value {
        Value::Array(values) => Value::Array(values.iter().map(schema_shape).collect()),
        Value::Object(values) => Value::Object(
            values
                .iter()
                .filter(|(key, _)| !matches!(key.as_str(), "default" | "required"))
                .map(|(key, value)| (key.clone(), schema_shape(value)))
                .collect(),
        ),
        _ => value.clone(),
    }
}

fn schema_identity(value: &Value) -> Value {
    match value {
        Value::Array(values) => Value::Array(values.iter().map(schema_identity).collect()),
        Value::Object(values) => {
            let mut identity = serde_json::Map::new();
            for (key, value) in values {
                if matches!(
                    key.as_str(),
                    "description" | "deprecated_message" | "examples" | "warnings"
                ) {
                    continue;
                }

                let value = if key == "enum" {
                    value.as_object().map_or_else(
                        || schema_identity(value),
                        |variants| {
                            Value::Object(
                                variants
                                    .keys()
                                    .map(|name| (name.clone(), Value::Null))
                                    .collect(),
                            )
                        },
                    )
                } else {
                    schema_identity(value)
                };
                identity.insert(key.clone(), value);
            }
            Value::Object(identity)
        }
        _ => value.clone(),
    }
}

struct CueDocument {
    data: Value,
    friendly_name: String,
    prefix: String,
    output_file: PathBuf,
}

pub fn run(schema_path: &Path) -> Result<()> {
    let schema_content = fs::read_to_string(schema_path)
        .with_context(|| format!("Failed to read schema file from {}", schema_path.display()))?;

    let root_schema: Value =
        serde_json::from_str(&schema_content).with_context(|| "Failed to parse schema JSON")?;

    let mut context = SchemaContext::new(root_schema.clone())?;
    let mut cue_definitions = CueDefinitions::from_schema(&mut context)?;
    let mut documents = Vec::new();

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
        documents.push(render_generated_component_schema(
            &mut context,
            schema_name,
            comp_type,
        )?);
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
            documents.push(render_component_schema(
                &mut context,
                schema_name,
                comp_type,
                comp_name,
            )?);
        }
    }

    // 3. Process top-level configuration fields (formerly "global options").
    // The standalone API schema (`generated/api.cue`) was retired in #24858; api
    // is now rendered as a top-level field with `group: "api"`.
    documents.push(render_generated_top_level_config_schema(
        &mut context,
        &root_schema,
    )?);

    cue_definitions
        .discover_reused_anonymous_values(documents.iter().map(|document| &document.data));

    for document in &documents {
        cue_definitions.count_references(&document.data);
    }
    cue_definitions.retain_reused();

    cue_definitions.reset_usage_counts();
    for document in &mut documents {
        cue_definitions.replace_references(&mut document.data);
    }
    cue_definitions.replace_references_in_used_definitions();
    cue_definitions.retain_used();

    for document in documents {
        import_json_as_cue(
            &context,
            Some(&cue_definitions),
            &document.data,
            &document.friendly_name,
            &document.prefix,
            &document.output_file,
        )?;
    }
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
        "[✓]   Wrote {friendly_name} schema to '{}'. ({} bytes)",
        json_output_file.display(),
        final_json.len()
    );
    debug!("[*] Importing {friendly_name} schema as Cue file...");

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
        "[✓]   Imported {friendly_name} schema to '{}'.",
        cue_output_file.display()
    );
    Ok(())
}

fn render_schema(
    unwrapped_resolved_schema: Value,
    friendly_name: &str,
    config_map_path: &[&str],
    cue_relative_path: &str,
) -> CueDocument {
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

    CueDocument {
        data: Value::Object(data),
        friendly_name: friendly_name.to_string(),
        prefix: format!("config-schema-base-{}-", config_map_path.join("-")),
        output_file: PathBuf::from("website/cue/reference").join(cue_relative_path),
    }
}

fn render_generated_component_schema(
    context: &mut SchemaContext,
    schema_name: &str,
    component_type: &str,
) -> Result<CueDocument> {
    let friendly_name = format!("generated {component_type} configuration");
    let unwrapped = context.unwrap_resolved_schema(schema_name, &friendly_name)?;
    let cue_path = format!("components/generated/{component_type}s.cue");

    Ok(render_schema(
        Value::Object(unwrapped),
        &friendly_name,
        &["generated", "components", &format!("{component_type}s")],
        &cue_path,
    ))
}

fn render_component_schema(
    context: &mut SchemaContext,
    schema_name: &str,
    component_type: &str,
    component_name: &str,
) -> Result<CueDocument> {
    let friendly_name = format!("'{component_name}' {component_type} configuration");
    let unwrapped = context.unwrap_resolved_schema(schema_name, &friendly_name)?;
    let cue_path = format!("components/{component_type}s/generated/{component_name}.cue");

    Ok(render_schema(
        Value::Object(unwrapped),
        &friendly_name,
        &[
            "generated",
            "components",
            &format!("{component_type}s"),
            component_name,
        ],
        &cue_path,
    ))
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

fn render_generated_top_level_config_schema(
    context: &mut SchemaContext,
    root_schema: &Value,
) -> Result<CueDocument> {
    let resolved_fields = resolve_top_level_config_fields(context, root_schema)?;

    Ok(CueDocument {
        data: json!({
            "generated": {
                "configuration": {
                    "configuration": Value::Object(resolved_fields),
                    "groups": top_level_group_metadata(),
                }
            }
        }),
        friendly_name: "configuration".to_string(),
        prefix: "config-schema-base-generated-configuration-".to_string(),
        output_file: PathBuf::from("website/cue/reference").join("generated/configuration.cue"),
    })
}

fn render_and_import_cue_definitions(
    context: &SchemaContext,
    cue_definitions: &CueDefinitions,
) -> Result<()> {
    let definitions_directory = PathBuf::from(CUE_DEFINITIONS_DIRECTORY);
    if definitions_directory.exists() {
        fs::remove_dir_all(&definitions_directory)?;
    }
    fs::create_dir_all(&definitions_directory)?;

    let definition_file_names =
        definition_file_names(cue_definitions.values.keys().map(String::as_str));

    for (name, value) in &cue_definitions.values {
        let data = json!({
            (CUE_DEFINITIONS_PLACEHOLDER_FIELD): {
                (name): value,
            },
        });
        let cue_output_file = definitions_directory.join(
            definition_file_names
                .get(name)
                .expect("every CUE definition has a filename"),
        );
        import_json_as_cue(
            context,
            Some(cue_definitions),
            &data,
            &format!("shared schema definition '{name}'"),
            "config-schema-cue-definition-",
            &cue_output_file,
        )?;

        let cue_output = fs::read_to_string(&cue_output_file)?;
        let cue_output = hide_cue_definitions_field(&cue_output)?;
        fs::write(&cue_output_file, cue_output)?;
    }

    Ok(())
}

fn definition_file_name(name: &str) -> String {
    if let Some(identity) = name.strip_prefix("derived::") {
        let (hint, digest) = identity.rsplit_once("::").unwrap_or(("", identity));
        let stem = if hint.is_empty() {
            "derived".to_string()
        } else {
            sanitized_definition_name(&format!("derived::{hint}"))
        };
        return format!("{stem}_{digest}.cue");
    }

    format!("{}.cue", sanitized_definition_name(name))
}

fn definition_file_names<'a>(names: impl IntoIterator<Item = &'a str>) -> HashMap<String, String> {
    let names = names.into_iter().collect::<Vec<_>>();
    let mut counts = HashMap::<String, usize>::new();
    for name in &names {
        *counts.entry(definition_file_name(name)).or_default() += 1;
    }

    names
        .into_iter()
        .map(|name| {
            let file_name = definition_file_name(name);
            if counts.get(&file_name) == Some(&1) {
                return (name.to_string(), file_name);
            }

            let stem = file_name
                .strip_suffix(".cue")
                .expect("definition filenames use the CUE extension");
            let digest = Sha256::digest(name.as_bytes());
            (
                name.to_string(),
                format!("{stem}-{}.cue", hex::encode(&digest[..4])),
            )
        })
        .collect()
}

fn sanitized_definition_name(name: &str) -> String {
    let mut stem = String::with_capacity(name.len());
    let mut previous_was_separator = false;
    for character in name.chars() {
        if character.is_ascii_alphanumeric() {
            stem.push(character.to_ascii_lowercase());
            previous_was_separator = false;
        } else if !previous_was_separator {
            stem.push('_');
            previous_was_separator = true;
        }
    }
    let mut stem = stem.trim_matches('_').to_string();
    if stem.is_empty() {
        stem.push_str("definition");
    }
    stem
}

fn hide_cue_definitions_field(cue_output: &str) -> Result<String> {
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
        let shared_name = "shared_options".to_string();
        let shared = json!({"object": {"options": {"limit": {"type": {"uint": {}}}}}});
        let unique_name = "unique_options".to_string();
        let unique = json!({"object": {"options": {"path": {"type": {"string": {}}}}}});
        let mut values = IndexMap::new();
        values.insert(shared_name.clone(), shared.clone());
        values.insert(unique_name.clone(), unique.clone());
        let mut usage_counts = IndexMap::new();
        usage_counts.insert(shared_name.clone(), 0);
        usage_counts.insert(unique_name.clone(), 0);
        let names_by_value = [
            (canonical_value(&shared), shared_name.clone()),
            (canonical_value(&unique), unique_name),
        ]
        .into_iter()
        .collect();
        let mut definitions = CueDefinitions {
            values,
            usage_counts,
            names_by_value,
        };
        let mut generated = json!({
            "first": {"type": shared},
            "second": {"type": shared},
            "only": {"type": unique},
        });

        definitions.count_references(&generated);
        definitions.retain_reused();
        definitions.reset_usage_counts();
        definitions.replace_references(&mut generated);
        definitions.retain_used();

        let marker = Value::String(format!("{CUE_REFERENCE_MARKER_PREFIX}shared_options"));
        assert_eq!(generated.pointer("/first/type"), Some(&marker));
        assert_eq!(generated.pointer("/second/type"), Some(&marker));
        assert_eq!(generated.pointer("/only/type"), Some(&unique));
        assert_eq!(
            definitions.values.keys().collect::<Vec<_>>(),
            vec![&shared_name]
        );
    }

    #[test]
    fn discovers_repeated_anonymous_object_values() {
        let shared = json!({
            "object": {
                "options": {
                    "codec": {
                        "description": "Codec to use.",
                        "required": false,
                        "type": {"string": {"default": "bytes"}},
                    },
                },
            },
        });
        let named = json!({
            "object": {
                "options": {
                    "codec": {
                        "description": "Codec to use.",
                        "required": true,
                        "type": {"string": {}},
                    },
                },
            },
        });
        let mut definitions = CueDefinitions {
            values: IndexMap::from([("DeserializerConfig".to_string(), named.clone())]),
            usage_counts: IndexMap::from([("DeserializerConfig".to_string(), 0)]),
            names_by_value: HashMap::from([(
                canonical_value(&named),
                "DeserializerConfig".to_string(),
            )]),
        };
        let mut generated = json!({
            "first": {"type": shared},
            "second": {"type": shared},
        });

        definitions.discover_reused_anonymous_values([&generated]);
        definitions.count_references(&generated);
        definitions.retain_reused();
        definitions.reset_usage_counts();
        definitions.replace_references(&mut generated);
        definitions.retain_used();

        let name = definitions.values.keys().next().unwrap();
        assert!(name.starts_with("derived::DeserializerConfig::"));
        let marker = Value::String(format!("{CUE_REFERENCE_MARKER_PREFIX}{name}"));
        assert_eq!(generated.pointer("/first/type"), Some(&marker));
        assert_eq!(generated.pointer("/second/type"), Some(&marker));
    }

    #[test]
    fn replaces_references_inside_used_definitions() {
        let inner = json!({
            "object": {
                "options": {
                    "value": {"required": true, "type": {"string": {}}},
                },
            },
        });
        let outer = json!({
            "object": {
                "options": {
                    "inner": {"required": true, "type": inner},
                },
            },
        });
        let mut definitions = CueDefinitions {
            values: IndexMap::from([
                ("Inner".to_string(), inner.clone()),
                ("Outer".to_string(), outer),
            ]),
            usage_counts: IndexMap::from([("Inner".to_string(), 0), ("Outer".to_string(), 1)]),
            names_by_value: HashMap::from([(canonical_value(&inner), "Inner".to_string())]),
        };

        definitions.replace_references_in_used_definitions();

        assert_eq!(
            definitions.values["Outer"].pointer("/object/options/inner/type"),
            Some(&Value::String(format!(
                "{CUE_REFERENCE_MARKER_PREFIX}Inner"
            )))
        );
        assert_eq!(definitions.usage_counts["Inner"], 1);
    }

    #[test]
    fn derived_definition_names_ignore_documentation() {
        let old = json!({
            "object": {
                "options": {
                    "codec": {
                        "description": "Old docs.",
                        "type": {"string": {"enum": {"bytes": "Old variant docs."}}},
                    },
                },
            },
        });
        let new = json!({
            "object": {
                "options": {
                    "codec": {
                        "description": "New docs.",
                        "type": {"string": {"enum": {"bytes": "New variant docs."}}},
                    },
                },
            },
        });

        assert_eq!(
            derived_definition_name(&old, Some("DeserializerConfig")),
            derived_definition_name(&new, Some("DeserializerConfig"))
        );
    }

    #[test]
    fn definition_file_names_are_stable_and_safe() {
        assert_eq!(
            definition_file_name("derived::bb2440a04988b7e322be398c"),
            "derived_bb2440a04988b7e322be398c.cue"
        );
        assert_eq!(
            definition_file_name(
                "derived::codecs::decoding::DeserializerConfig::bb2440a04988b7e322be398c"
            ),
            "derived_codecs_decoding_deserializerconfig_bb2440a04988b7e322be398c.cue"
        );

        let name = "core::option::Option<vector_core::tls::settings::TlsConfig>";
        let filename = definition_file_name(name);

        assert_eq!(
            filename,
            "core_option_option_vector_core_tls_settings_tlsconfig.cue"
        );
        assert!(
            Path::new(&filename)
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("cue"))
        );
        assert!(
            filename
                .chars()
                .all(|character| character.is_ascii_lowercase()
                    || character.is_ascii_digit()
                    || matches!(character, '_' | '-' | '.'))
        );

        let colliding = definition_file_names(["example::Config", "example<Config>"]);
        let first = &colliding["example::Config"];
        let second = &colliding["example<Config>"];
        assert!(first.starts_with("example_config-"));
        assert!(second.starts_with("example_config-"));
        assert_ne!(first, second);
    }

    #[test]
    fn rewrites_imported_markers_as_cue_expressions() {
        let mut values = IndexMap::new();
        values.insert("shared_options".to_string(), json!({}));
        let definitions = CueDefinitions {
            values,
            usage_counts: IndexMap::new(),
            names_by_value: HashMap::new(),
        };
        let mut cue = format!("type: \"{CUE_REFERENCE_MARKER_PREFIX}shared_options\"\n");

        definitions.rewrite_reference_markers(&mut cue).unwrap();

        assert_eq!(cue, "type: _schemaDefinitions[\"shared_options\"]\n");
    }

    #[test]
    fn hides_imported_definition_storage() {
        let cue =
            format!("package metadata\n\n{CUE_DEFINITIONS_PLACEHOLDER_FIELD}: shared: {{}}\n");

        assert_eq!(
            hide_cue_definitions_field(&cue).unwrap(),
            "package metadata\n\n_schemaDefinitions: shared: {}\n"
        );
    }
}
