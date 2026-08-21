use std::{
    collections::HashSet,
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, anyhow};
use clap::Args;
use serde_json::{Map, Value};
use tempfile::Builder;

use crate::{
    app,
    utils::paths::{find_repo_root, npm_tool_path, resolve_repo_relative_path},
};

use super::docs_json;

const DEFAULT_EXAMPLES_DIR: &str = "website/generated/example-configs";
const DEFAULT_DOCS_OUTPUT: &str = "website/data/docs.json";

/// Generate component configuration examples from the final CUE documentation model.
#[derive(Args, Debug)]
#[command()]
pub struct Cli {
    /// Directory where generated YAML examples are written, relative to the repository root.
    #[arg(default_value = DEFAULT_EXAMPLES_DIR)]
    output: PathBuf,
}

impl Cli {
    pub fn exec(self) -> Result<()> {
        let repo_root = find_repo_root()?;
        let output =
            resolve_repo_relative_path(&repo_root, &self.output, "Example output directory")?;
        let docs = docs_json::render_docs(&repo_root)?;
        let mut docs: Value =
            serde_json::from_slice(&docs).context("CUE did not produce valid JSON")?;

        generate(&mut docs, &output)?;
        format_examples(&repo_root, &output)?;
        sync_formatted_yaml_examples(&mut docs, &output)?;
        let rendered =
            serde_json::to_vec(&docs).context("Failed to serialize documentation JSON")?;
        docs_json::write_docs(&repo_root.join(DEFAULT_DOCS_OUTPUT), &rendered)
    }
}

pub(crate) fn generate(docs: &mut Value, output: &Path) -> Result<()> {
    let parent = output.parent().ok_or_else(|| {
        anyhow!(
            "Example output directory has no parent: {}",
            output.display()
        )
    })?;
    fs::create_dir_all(parent).with_context(|| format!("Failed to create {}", parent.display()))?;
    let temporary = Builder::new()
        .prefix("component-examples-")
        .tempdir_in(parent)
        .with_context(|| {
            format!(
                "Failed to create a temporary directory in {}",
                parent.display()
            )
        })?;

    generate_into(docs, temporary.path())?;

    if output.exists() {
        fs::remove_dir_all(output)
            .with_context(|| format!("Failed to remove {}", output.display()))?;
    }
    fs::rename(temporary.keep(), output)
        .with_context(|| format!("Failed to replace {}", output.display()))
}

pub(crate) fn format_examples(repo_root: &Path, output: &Path) -> Result<()> {
    info!("Formatting generated component examples with prettier...");
    let prettier = npm_tool_path(repo_root, "prettier")?;
    app::exec(
        prettier,
        [
            OsStr::new("--ignore-path"),
            OsStr::new(".prettierignore"),
            OsStr::new("--write"),
            output.as_os_str(),
        ],
        true,
    )
}

fn sync_formatted_yaml_examples(docs: &mut Value, output: &Path) -> Result<()> {
    let components = object_mut(docs, "documentation model")?
        .get_mut("components")
        .ok_or_else(|| anyhow!("Documentation model has no components"))?;
    let components = object_mut(components, "components")?;

    for (kind, components_of_kind) in components {
        let components_of_kind = object_mut(components_of_kind, "component kind")?;
        for (component_type, component) in components_of_kind {
            let component = object_mut(component, "component")?;
            let example_configs = component
                .get_mut("example_configs")
                .ok_or_else(|| anyhow!("{kind}/{component_type} has no example configs"))?;
            let example_configs = object_mut(example_configs, "example configs")?;

            for variant in ["minimal", "advanced"] {
                let path = output
                    .join(kind)
                    .join(component_type)
                    .join(format!("{variant}.yaml"));
                let yaml = fs::read_to_string(&path).with_context(|| {
                    format!("Failed to read formatted example {}", path.display())
                })?;
                let formats = example_configs
                    .get_mut(variant)
                    .ok_or_else(|| anyhow!("{kind}/{component_type} has no {variant} example"))?;
                object_mut(formats, "example formats")?
                    .insert("yaml".to_owned(), Value::String(yaml));
            }
        }
    }

    Ok(())
}

fn generate_into(docs: &mut Value, output: &Path) -> Result<()> {
    let components = object_mut(docs, "documentation model")?
        .get_mut("components")
        .ok_or_else(|| anyhow!("Documentation model has no components"))?;
    let components = object_mut(components, "components")?;

    for (kind, components_of_kind) in components {
        let components_of_kind = object_mut(components_of_kind, "component kind")?;
        for (component_type, component) in components_of_kind {
            let component = object_mut(component, "component")?;
            let configuration = component
                .get("configuration")
                .and_then(Value::as_object)
                .ok_or_else(|| anyhow!("{kind}/{component_type} has no configuration"))?;

            let minimal_params =
                make_example_params(configuration, selected_for_minimal, selected_for_minimal);
            let advanced_params = make_example_params(
                configuration,
                |_| true,
                |param| {
                    selected_for_minimal(param)
                        || relevant_when(param).is_some()
                        || has_field_examples(param)
                },
            );
            let use_case_examples = make_use_case_examples(component)?;
            let examples =
                make_component_examples(kind, component_type, minimal_params, advanced_params);

            component.insert("examples".to_owned(), use_case_examples);
            component.insert("example_configs".to_owned(), example_formats(&examples)?);

            for (variant, example) in examples {
                let yaml = to_yaml(&example)?;
                let path = output
                    .join(kind)
                    .join(component_type)
                    .join(format!("{variant}.yaml"));
                let directory = path
                    .parent()
                    .ok_or_else(|| anyhow!("Example path has no parent: {}", path.display()))?;
                fs::create_dir_all(directory)
                    .with_context(|| format!("Failed to create {}", directory.display()))?;
                fs::write(&path, yaml)
                    .with_context(|| format!("Failed to write {}", path.display()))?;
            }
        }
    }

    Ok(())
}

fn selected_for_minimal(param: &Value) -> bool {
    param
        .get("required")
        .and_then(Value::as_bool)
        .unwrap_or(false)
        || param
            .get("minimal")
            .and_then(Value::as_bool)
            .unwrap_or(false)
}

fn has_field_examples(param: &Value) -> bool {
    param
        .get("type")
        .and_then(Value::as_object)
        .is_some_and(|types| {
            types.values().any(|type_info| {
                type_info
                    .get("examples")
                    .and_then(Value::as_array)
                    .is_some_and(|examples| !examples.is_empty())
            })
        })
}

fn relevant_when(param: &Value) -> Option<&str> {
    param.get("relevant_when").and_then(Value::as_str)
}

fn make_component_examples(
    kind: &str,
    component_type: &str,
    minimal_params: Map<String, Value>,
    advanced_params: Map<String, Value>,
) -> [(String, Value); 2] {
    let key = format!("my_{}_id", kind.trim_end_matches('s'));
    let with_inputs = matches!(kind, "sinks" | "transforms");
    let make_example = |params: Map<String, Value>| {
        let mut config = Map::new();
        config.insert("type".to_owned(), Value::String(component_type.to_owned()));
        if with_inputs {
            config.insert(
                "inputs".to_owned(),
                Value::Array(vec![Value::String("my-source-or-transform-id".to_owned())]),
            );
        }
        config.extend(params);

        let mut ids = Map::new();
        ids.insert(key.clone(), Value::Object(config));
        let mut root = Map::new();
        root.insert(kind.to_owned(), Value::Object(ids));
        Value::Object(root)
    };

    [
        ("minimal".to_owned(), make_example(minimal_params)),
        ("advanced".to_owned(), make_example(advanced_params)),
    ]
}

fn example_formats(examples: &[(String, Value); 2]) -> Result<Value> {
    let mut variants = Map::new();
    for (variant, example) in examples {
        let mut formats = Map::new();
        formats.insert("toml".to_owned(), Value::String(toml::to_string(example)?));
        formats.insert("yaml".to_owned(), Value::String(to_yaml(example)?));
        formats.insert(
            "json".to_owned(),
            Value::String(serde_json::to_string_pretty(example)?),
        );
        variants.insert(variant.clone(), Value::Object(formats));
    }
    Ok(Value::Object(variants))
}

fn make_use_case_examples(component: &Map<String, Value>) -> Result<Value> {
    let Some(examples) = component.get("examples").and_then(Value::as_array) else {
        return Ok(Value::Null);
    };

    let kind = component
        .get("kind")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("Component example is missing kind"))?;
    let component_type = component
        .get("type")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("Component example is missing type"))?;
    let key = format!("my_{kind}_id");
    let mut rendered = Vec::with_capacity(examples.len());

    for example in examples {
        let example = object(example, "component example")?;
        let configuration = example
            .get("configuration")
            .and_then(Value::as_object)
            .ok_or_else(|| anyhow!("Component example is missing configuration"))?;
        let extra = configuration
            .iter()
            .filter(|(_, value)| !value.is_null())
            .map(|(key, value)| (key.clone(), value.clone()));

        let mut config = Map::new();
        config.insert("type".to_owned(), Value::String(component_type.to_owned()));
        if matches!(kind, "transform" | "sink") {
            config.insert(
                "inputs".to_owned(),
                Value::Array(vec![Value::String("my-source-or-transform-id".to_owned())]),
            );
        }
        config.extend(extra);
        let mut ids = Map::new();
        ids.insert(key.clone(), Value::Object(config));
        let mut root = Map::new();
        root.insert(format!("{kind}s"), Value::Object(ids));
        let configuration = Value::Object(root);

        let output = example.get("output").map_or(Value::Null, |output| {
            output
                .get("log")
                .or_else(|| output.get("metric"))
                .cloned()
                .unwrap_or_else(|| output.clone())
        });
        let mut use_case = Map::new();
        for field in ["title", "description", "input"] {
            use_case.insert(
                field.to_owned(),
                example.get(field).cloned().unwrap_or(Value::Null),
            );
        }
        use_case.insert("output".to_owned(), output);
        let mut formats = Map::new();
        formats.insert(
            "toml".to_owned(),
            Value::String(toml::to_string(&configuration)?),
        );
        formats.insert("yaml".to_owned(), Value::String(to_yaml(&configuration)?));
        formats.insert(
            "json".to_owned(),
            Value::String(serde_json::to_string_pretty(&configuration)?),
        );
        use_case.insert("configuration".to_owned(), Value::Object(formats));
        rendered.push(Value::Object(use_case));
    }

    Ok(Value::Array(rendered))
}

fn make_example_params<F, G>(
    params: &Map<String, Value>,
    filter: F,
    deep_filter: G,
) -> Map<String, Value>
where
    F: Fn(&Value) -> bool,
    G: Fn(&Value) -> bool,
{
    let discriminators = params
        .iter()
        .filter(|(_, param)| selected_for_minimal(param) && relevant_when(param).is_none())
        .filter_map(|(key, param)| {
            get_example_value(param, |_| false).map(|value| (key.clone(), value_to_string(&value)))
        })
        .collect::<std::collections::HashMap<_, _>>();

    let mut selected = HashSet::new();
    let mut groups = HashSet::new();
    for param in params.values() {
        let Some(group) = param
            .get("required_one_of")
            .and_then(Value::as_array)
            .filter(|group| !group.is_empty())
        else {
            continue;
        };
        let group_key = group[0].as_str().unwrap_or_default();
        if !groups.insert(group_key) {
            continue;
        }
        let member = group
            .iter()
            .filter_map(Value::as_str)
            .find(|member| {
                params
                    .get(*member)
                    .is_some_and(|param| get_example_value(param, |_| false).is_some())
            })
            .or_else(|| group[0].as_str());
        if let Some(member) = member {
            selected.insert(member);
        }
    }

    params
        .iter()
        .filter(|(key, param)| {
            let in_group = param
                .get("required_one_of")
                .and_then(Value::as_array)
                .is_some_and(|group| !group.is_empty());
            (!in_group || selected.contains(key.as_str()))
                && (filter(param) || selected.contains(key.as_str()))
                && matches_relevant_when(param, &discriminators)
        })
        .filter_map(|(key, param)| {
            get_example_value(param, &deep_filter).map(|value| (key.clone(), value))
        })
        .collect()
}

fn matches_relevant_when(
    param: &Value,
    discriminators: &std::collections::HashMap<String, String>,
) -> bool {
    let Some(condition) = relevant_when(param) else {
        return true;
    };
    let Some((key, rest)) = condition.split_once('=') else {
        return true;
    };
    let key = key.trim();
    let value = rest
        .trim()
        .strip_prefix('"')
        .and_then(|rest| rest.split('"').next());
    value.is_none_or(|value| {
        discriminators
            .get(key)
            .is_some_and(|selected| selected == value)
    })
}

fn get_example_value<F>(param: &Value, deep_filter: F) -> Option<Value>
where
    F: Fn(&Value) -> bool,
{
    let types = param.get("type")?.as_object()?;
    let mut value = None;
    for (kind, type_info) in types {
        if matches!(kind.as_str(), "array" | "object") {
            if let Some(item_types) = type_info
                .get("items")
                .and_then(|items| items.get("type"))
                .and_then(Value::as_object)
            {
                for (item_kind, item_type) in item_types {
                    if matches!(item_kind.as_str(), "array" | "object") {
                        let options = item_type.get("options").and_then(Value::as_object)?;
                        let mut object = Map::new();
                        for (key, option) in
                            options.iter().filter(|(_, option)| deep_filter(option))
                        {
                            let option_types = option.get("type")?.as_object()?;
                            for option_type in option_types.values() {
                                let candidate = if item_kind == "array" {
                                    get_array_value(option_type)
                                } else {
                                    get_value(option_type)
                                };
                                if let Some(candidate) = candidate {
                                    object.insert(key.clone(), candidate);
                                }
                            }
                        }
                        if !object.is_empty() {
                            value = Some(if kind == "array" {
                                Value::Array(vec![Value::Object(object)])
                            } else {
                                Value::Object(object)
                            });
                        } else if let Some(example) = first_example(item_type) {
                            value = Some(if kind == "array" {
                                Value::Array(vec![example])
                            } else {
                                example
                            });
                        }
                    } else if kind == "array" {
                        value = get_array_value(item_type);
                    } else {
                        value = get_value(item_type);
                    }
                }
            } else if let Some(example) = first_example(type_info) {
                if example.is_object() {
                    let stripped = strip_nulls(example);
                    if stripped
                        .as_object()
                        .is_some_and(|object| !object.is_empty())
                    {
                        value = Some(stripped);
                    }
                } else {
                    value = Some(example);
                }
            } else if type_info
                .get("options")
                .and_then(Value::as_object)
                .is_some()
                && selected_for_minimal(param)
            {
                let object =
                    build_from_options(type_info.get("options").and_then(Value::as_object)?);
                if !object.is_empty() {
                    value = Some(Value::Object(object));
                }
            } else {
                value = get_value(type_info);
            }
        } else if kind == "condition"
            && selected_for_minimal(param)
            && type_info
                .get("syntaxes")
                .and_then(Value::as_array)
                .is_some_and(|syntaxes| !syntaxes.is_empty())
        {
            let syntaxes = type_info.get("syntaxes").and_then(Value::as_array)?;
            let syntax = syntaxes
                .iter()
                .find(|syntax| syntax.get("name").and_then(Value::as_str) == Some("vrl"))
                .unwrap_or(&syntaxes[0]);
            if let Some(example) = syntax.get("example").and_then(Value::as_str) {
                value = Some(Value::Object(Map::from_iter([
                    ("type".to_owned(), Value::String("vrl".to_owned())),
                    ("source".to_owned(), Value::String(example.to_owned())),
                ])));
            }
        } else if kind == "bool" {
            value = get_value(type_info).or(Some(Value::Bool(false)));
        } else {
            value = get_value(type_info);
        }
    }
    value
}

fn build_from_options(options: &Map<String, Value>) -> Map<String, Value> {
    let mut object = Map::new();
    for (key, option) in options {
        if key == "*"
            || !option
                .get("required")
                .and_then(Value::as_bool)
                .unwrap_or(false)
            || relevant_when(option).is_some()
        {
            continue;
        }
        let Some(types) = option.get("type").and_then(Value::as_object) else {
            continue;
        };
        for type_info in types.values() {
            if let Some(nested_options) = type_info.get("options").and_then(Value::as_object) {
                if let Some(example) = first_example(type_info) {
                    object.insert(key.clone(), example);
                } else {
                    let nested = build_from_options(nested_options);
                    if !nested.is_empty() {
                        object.insert(key.clone(), Value::Object(nested));
                    }
                }
            } else if let Some(value) = get_value_preferring_simple(type_info, options) {
                object.insert(key.clone(), value);
            }
        }
    }
    object
}

fn get_value_preferring_simple(value: &Value, options: &Map<String, Value>) -> Option<Value> {
    let Some(enum_values) = value.get("enum").and_then(Value::as_object) else {
        return get_value(value);
    };
    let simple = |key: &str| {
        !options.values().any(|option| {
            option
                .get("required")
                .and_then(Value::as_bool)
                .unwrap_or(false)
                && relevant_when(option)
                    .is_some_and(|condition| condition.contains(&format!("\"{key}\"")))
        })
    };
    let keys = enum_values.keys().collect::<Vec<_>>();
    let simple_keys = keys
        .iter()
        .copied()
        .filter(|key| simple(key))
        .collect::<Vec<_>>();
    let preferred = ["json", "text", "logfmt"]
        .into_iter()
        .find(|key| enum_values.contains_key(*key) && simple(key))
        .map(str::to_owned)
        .or_else(|| {
            if simple_keys.iter().all(|key| numeric_key(key)) {
                simple_keys
                    .iter()
                    .filter_map(|key| {
                        key.parse::<u64>()
                            .ok()
                            .map(|number| (number, (*key).to_owned()))
                    })
                    .max_by_key(|(number, _)| *number)
                    .map(|(_, key)| key)
            } else {
                simple_keys.first().map(|key| (*key).to_owned())
            }
        });
    truthy(value.get("default"))
        .cloned()
        .or_else(|| preferred.map(Value::String))
}

fn get_array_value(value: &Value) -> Option<Value> {
    let enum_value = value
        .get("enum")
        .and_then(Value::as_object)
        .and_then(|values| values.keys().next())
        .map(|value| Value::Array(vec![Value::String(value.to_owned())]));
    let examples = first_example(value).map(|value| Value::Array(vec![value]));
    safe_truthy(value.get("default"))
        .cloned()
        .or_else(|| examples.and_then(|value| truthy(Some(&value)).cloned()))
        .or(enum_value)
}

fn get_value(value: &Value) -> Option<Value> {
    let enum_values = value.get("enum").and_then(Value::as_object);
    if enum_values
        .is_some_and(|values| !values.is_empty() && values.keys().all(|key| numeric_key(key)))
    {
        let maximum = enum_values?
            .keys()
            .filter_map(|key| key.parse::<u64>().ok())
            .max()
            .map(|number| Value::String(number.to_string()));
        return safe_truthy(value.get("default")).cloned().or(maximum);
    }
    let enum_value = enum_values
        .and_then(|values| values.keys().next())
        .map(|key| Value::String(key.clone()));
    safe_truthy(value.get("default"))
        .cloned()
        .or_else(|| first_example(value).and_then(|value| truthy(Some(&value)).cloned()))
        .or(enum_value)
}

fn first_example(value: &Value) -> Option<Value> {
    value
        .get("examples")
        .and_then(Value::as_array)
        .and_then(|examples| examples.first())
        .filter(|value| is_safe(value))
        .cloned()
}

fn is_safe(value: &Value) -> bool {
    value
        .as_f64()
        .is_none_or(|value| value.abs() <= 9_007_199_254_740_991.0)
}

fn truthy(value: Option<&Value>) -> Option<&Value> {
    value.filter(|value| match value {
        Value::Null => false,
        Value::Bool(value) => *value,
        Value::Number(value) => value.as_f64().is_none_or(|value| value != 0.0),
        Value::String(value) => !value.is_empty(),
        Value::Array(_) | Value::Object(_) => true,
    })
}

fn safe_truthy(value: Option<&Value>) -> Option<&Value> {
    value
        .filter(|value| is_safe(value))
        .and_then(|value| truthy(Some(value)))
}

fn numeric_key(key: &str) -> bool {
    !key.is_empty() && key.bytes().all(|byte| byte.is_ascii_digit())
}

fn value_to_string(value: &Value) -> String {
    match value {
        Value::String(value) => value.clone(),
        Value::Number(value) => value.to_string(),
        Value::Bool(value) => value.to_string(),
        Value::Null => "null".to_owned(),
        Value::Array(_) | Value::Object(_) => "[object Object]".to_owned(),
    }
}

fn strip_nulls(value: Value) -> Value {
    match value {
        Value::Object(object) => Value::Object(
            object
                .into_iter()
                .filter_map(|(key, value)| {
                    (!value.is_null()).then(|| {
                        let value = strip_nulls(value);
                        (key, value)
                    })
                })
                .filter(|(_, value)| {
                    !value.is_object() || !value.as_object().is_some_and(Map::is_empty)
                })
                .collect(),
        ),
        value => value,
    }
}

fn to_yaml(value: &Value) -> Result<String> {
    let mut value = value.clone();
    let mut escaped_strings = Vec::new();
    normalize_yaml_strings(&mut value, &mut escaped_strings);

    let mut yaml = serde_yaml::to_string(&value).context("Failed to serialize YAML example")?;
    for (index, value) in escaped_strings.into_iter().enumerate() {
        let placeholder = yaml_string_placeholder(index);
        let escaped = serde_json::to_string(&value).context("Failed to escape YAML string")?;
        yaml = yaml.replacen(&placeholder, &escaped, 1);
    }
    Ok(yaml)
}

// serde_yaml renders control-only strings as literal blocks, making a newline delimiter an empty
// block and normalizing carriage returns away. JSON strings are valid YAML double-quoted scalars.
// Lua examples use tabs only for indentation; use spaces so serde_yaml and Prettier retain their
// readable block-scalar form, matching the legacy generator.
fn normalize_yaml_strings(value: &mut Value, escaped_strings: &mut Vec<String>) {
    match value {
        Value::String(string)
            if string.contains('\r')
                || (!string.is_empty() && string.chars().all(char::is_control)) =>
        {
            let placeholder = yaml_string_placeholder(escaped_strings.len());
            escaped_strings.push(std::mem::replace(string, placeholder));
        }
        Value::String(string) if string.contains('\t') => {
            *string = string.replace('\t', "  ");
        }
        Value::Array(values) => {
            for value in values {
                normalize_yaml_strings(value, escaped_strings);
            }
        }
        Value::Object(values) => {
            for value in values.values_mut() {
                normalize_yaml_strings(value, escaped_strings);
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
    }
}

fn yaml_string_placeholder(index: usize) -> String {
    format!("__VECTOR_YAML_ESCAPED_STRING_{index}__")
}

fn object<'a>(value: &'a Value, description: &str) -> Result<&'a Map<String, Value>> {
    value
        .as_object()
        .ok_or_else(|| anyhow!("Expected {description} to be an object"))
}

fn object_mut<'a>(value: &'a mut Value, description: &str) -> Result<&'a mut Map<String, Value>> {
    value
        .as_object_mut()
        .ok_or_else(|| anyhow!("Expected {description} to be an object"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn docs_embed_formatted_yaml_examples() {
        let temporary = tempdir().unwrap();
        let output = temporary.path().join("examples");
        let example_dir = output.join("sources/demo");
        fs::create_dir_all(&example_dir).unwrap();
        fs::write(
            example_dir.join("minimal.yaml"),
            "sources:\n  demo: minimal\n",
        )
        .unwrap();
        fs::write(
            example_dir.join("advanced.yaml"),
            "sources:\n  demo: advanced\n",
        )
        .unwrap();
        let mut docs = serde_json::json!({
            "components": {
                "sources": {
                    "demo": {
                        "example_configs": {
                            "minimal": { "toml": "minimal", "yaml": "unformatted", "json": "{}" },
                            "advanced": { "toml": "advanced", "yaml": "unformatted", "json": "{}" }
                        }
                    }
                }
            }
        });

        sync_formatted_yaml_examples(&mut docs, &output).unwrap();

        assert_eq!(
            docs["components"]["sources"]["demo"]["example_configs"]["minimal"]["yaml"],
            "sources:\n  demo: minimal\n"
        );
        assert_eq!(
            docs["components"]["sources"]["demo"]["example_configs"]["advanced"]["yaml"],
            "sources:\n  demo: advanced\n"
        );
        assert_eq!(
            docs["components"]["sources"]["demo"]["example_configs"]["minimal"]["toml"],
            "minimal"
        );
    }

    #[test]
    fn yaml_preserves_control_only_strings() {
        let yaml = to_yaml(&Value::String("\r\n".to_owned())).unwrap();

        assert_eq!(yaml, "\"\\r\\n\"\n");
        assert_eq!(serde_yaml::from_str::<String>(&yaml).unwrap(), "\r\n");

        let yaml = to_yaml(&Value::String("\n".to_owned())).unwrap();
        assert_eq!(yaml, "\"\\n\"\n");
        assert_eq!(serde_yaml::from_str::<String>(&yaml).unwrap(), "\n");
    }

    #[test]
    fn yaml_uses_spaces_for_lua_indentation() {
        let yaml = to_yaml(&Value::String(
            "function init()\n\tcount = 0\nend".to_owned(),
        ))
        .unwrap();

        assert!(yaml.starts_with("|-\n"));
        assert_eq!(
            serde_yaml::from_str::<String>(&yaml).unwrap(),
            "function init()\n  count = 0\nend"
        );
    }
}
