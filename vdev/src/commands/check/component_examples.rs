use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::Mutex,
};

use anyhow::{Context, Result, anyhow, bail};
use clap::Args;
use serde_json::Value;
use serde_yaml::{Mapping, Value as YamlValue};
use tempfile::Builder;

use crate::{
    commands::build::docs_json,
    utils::{git, paths::find_repo_root},
};

const EXAMPLES_DIR: &str = "website/generated/example-configs";

/// Check that generated component configuration examples are up-to-date and valid.
#[derive(Args, Debug)]
#[command()]
pub struct Cli {}

impl Cli {
    pub fn exec(self) -> Result<()> {
        let repo_root = find_repo_root()?;
        check_generated_examples()?;
        let docs = docs_json::render_docs(&repo_root)?;
        let docs: Value =
            serde_json::from_slice(&docs).context("CUE did not produce valid JSON")?;

        validate_examples(&repo_root, &repo_root.join(EXAMPLES_DIR), &docs)
    }
}

fn check_generated_examples() -> Result<()> {
    let changed_examples = component_example_changes(git::get_files_changed_from_head()?);
    if changed_examples.is_empty() {
        return Ok(());
    }

    println!("Found out-of-sync component examples in this branch:");
    for file in changed_examples {
        println!(" - {file}");
    }
    bail!("Run `make generate-docs` locally to update your branch and commit the changes.")
}

fn component_example_changes(files: Vec<String>) -> Vec<String> {
    files
        .into_iter()
        .filter(|file| file.starts_with(EXAMPLES_DIR))
        .collect()
}

struct ValidationCase {
    key: String,
    config: YamlValue,
}

fn validate_examples(repo_root: &Path, examples_dir: &Path, docs: &Value) -> Result<()> {
    let (cases, total, skipped) = validation_cases(examples_dir, docs)?;
    let vector_bin = vector_binary(repo_root)?;
    let jobs = std::env::var("VALIDATE_CONFIG_EXAMPLES_JOBS")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|jobs| *jobs > 0)
        .unwrap_or(4)
        .min(cases.len().max(1));
    let queue = Mutex::new(cases.iter());
    let failures = Mutex::new(Vec::new());

    std::thread::scope(|scope| {
        for _ in 0..jobs {
            scope.spawn(|| {
                loop {
                    let case = { queue.lock().expect("Validation queue lock poisoned").next() };
                    let Some(case) = case else {
                        break;
                    };
                    if let Err(error) = validate_case(repo_root, &vector_bin, case) {
                        failures
                            .lock()
                            .expect("Validation failure lock poisoned")
                            .push(format!("FAIL {}: {error:#}", case.key));
                    }
                }
            });
        }
    });

    let failures = failures
        .into_inner()
        .expect("Validation failure lock poisoned");
    println!(
        "Validated {} examples ({skipped} skipped).",
        total - skipped
    );
    if failures.is_empty() {
        println!("All examples passed.");
        return Ok(());
    }
    for failure in &failures {
        println!("{failure}");
    }
    bail!("{} validation failure(s).", failures.len())
}

fn validation_cases(
    examples_dir: &Path,
    docs: &Value,
) -> Result<(Vec<ValidationCase>, usize, usize)> {
    let components = docs
        .get("components")
        .and_then(Value::as_object)
        .ok_or_else(|| anyhow!("Documentation model has no components"))?;
    let mut cases = Vec::new();
    let mut total = 0;
    let mut skipped = 0;

    for (kind, components_of_kind) in components {
        let components_of_kind = components_of_kind
            .as_object()
            .ok_or_else(|| anyhow!("Component kind {kind} is not an object"))?;
        for (component_type, component) in components_of_kind {
            for variant in ["minimal", "advanced"] {
                total += 1;
                let key = format!("{kind}/{component_type} ({variant})");
                let path = examples_dir
                    .join(kind)
                    .join(component_type)
                    .join(format!("{variant}.yaml"));
                let yaml = fs::read_to_string(&path)
                    .with_context(|| format!("Could not read {}", path.display()))?;
                let yaml = serde_yaml::from_str(&yaml)
                    .with_context(|| format!("YAML parse error for {key}"))?;

                let Some(config) = wrap_config(kind, yaml, component)? else {
                    skipped += 1;
                    continue;
                };
                cases.push(ValidationCase { key, config });
            }
        }
    }

    Ok((cases, total, skipped))
}

fn wrap_config(
    kind: &str,
    component_yaml: YamlValue,
    component: &Value,
) -> Result<Option<YamlValue>> {
    match kind {
        "sources" => wrap_source(&component_yaml, component).map(Some),
        "transforms" => wrap_transform(&component_yaml, component),
        "sinks" => wrap_sink(&component_yaml, component),
        _ => Ok(Some(component_yaml)),
    }
}

fn wrap_source(component_yaml: &YamlValue, component: &Value) -> Result<YamlValue> {
    let mut root = yaml_mapping(component_yaml, "source example")?;
    let sources = yaml_mapping(
        mapping_value(&root, "sources")
            .ok_or_else(|| anyhow!("Source example has no sources section"))?,
        "source example sources",
    )?;
    let source_key = mapping_key(&sources, "source example")?;
    let mut sinks = Mapping::new();

    let outputs = component.get("outputs").and_then(Value::as_array);
    let has_named_outputs = outputs
        .and_then(|outputs| outputs.first())
        .and_then(|output| output.get("name"))
        .and_then(Value::as_str)
        .is_some_and(|name| name != "<component_id>");
    if has_named_outputs {
        for output in outputs.expect("Named outputs require an output list") {
            let name = output
                .get("name")
                .and_then(Value::as_str)
                .ok_or_else(|| anyhow!("Source output has no name"))?;
            sinks.insert(
                YamlValue::String(format!("_validate_sink_{name}")),
                blackhole(&format!("{source_key}.{name}")),
            );
        }
    } else {
        sinks.insert(
            YamlValue::String("_validate_sink".to_owned()),
            blackhole(&source_key),
        );
    }
    root.insert(
        YamlValue::String("sinks".to_owned()),
        YamlValue::Mapping(sinks),
    );
    Ok(YamlValue::Mapping(root))
}

fn wrap_transform(component_yaml: &YamlValue, component: &Value) -> Result<Option<YamlValue>> {
    let Some(source) = validation_source(component) else {
        return Ok(None);
    };
    let root = yaml_mapping(component_yaml, "transform example")?;
    let transforms = yaml_mapping(
        mapping_value(&root, "transforms")
            .ok_or_else(|| anyhow!("Transform example has no transforms section"))?,
        "transform example transforms",
    )?;
    let transform_key = mapping_key(&transforms, "transform example")?;
    let mut transform = yaml_mapping(
        mapping_value(&transforms, &transform_key)
            .ok_or_else(|| anyhow!("Transform {transform_key} is missing"))?,
        "transform configuration",
    )?;
    transform.insert(
        YamlValue::String("inputs".to_owned()),
        yaml_list(["_validate_source"]),
    );

    let named_outputs = route_outputs(&transform);
    let mut sinks = Mapping::new();
    if named_outputs.is_empty() {
        sinks.insert(
            YamlValue::String("_validate_sink".to_owned()),
            blackhole(&transform_key),
        );
    } else {
        for output in named_outputs {
            sinks.insert(
                YamlValue::String(format!("_validate_sink_{output}")),
                blackhole(&format!("{transform_key}.{output}")),
            );
        }
    }

    let mut transforms = Mapping::new();
    transforms.insert(
        YamlValue::String(transform_key),
        YamlValue::Mapping(transform),
    );
    let mut sources = Mapping::new();
    sources.insert(YamlValue::String("_validate_source".to_owned()), source);
    Ok(Some(YamlValue::Mapping(Mapping::from_iter([
        (
            YamlValue::String("sources".to_owned()),
            YamlValue::Mapping(sources),
        ),
        (
            YamlValue::String("transforms".to_owned()),
            YamlValue::Mapping(transforms),
        ),
        (
            YamlValue::String("sinks".to_owned()),
            YamlValue::Mapping(sinks),
        ),
    ]))))
}

fn wrap_sink(component_yaml: &YamlValue, component: &Value) -> Result<Option<YamlValue>> {
    let Some(source) = validation_source(component) else {
        return Ok(None);
    };
    let root = yaml_mapping(component_yaml, "sink example")?;
    let sinks = yaml_mapping(
        mapping_value(&root, "sinks")
            .ok_or_else(|| anyhow!("Sink example has no sinks section"))?,
        "sink example sinks",
    )?;
    let sink_key = mapping_key(&sinks, "sink example")?;
    let mut sink = yaml_mapping(
        mapping_value(&sinks, &sink_key).ok_or_else(|| anyhow!("Sink {sink_key} is missing"))?,
        "sink configuration",
    )?;
    sink.insert(
        YamlValue::String("inputs".to_owned()),
        yaml_list(["_validate_source"]),
    );

    let mut sources = Mapping::new();
    sources.insert(YamlValue::String("_validate_source".to_owned()), source);
    let mut sinks = Mapping::new();
    sinks.insert(YamlValue::String(sink_key), YamlValue::Mapping(sink));
    Ok(Some(YamlValue::Mapping(Mapping::from_iter([
        (
            YamlValue::String("sources".to_owned()),
            YamlValue::Mapping(sources),
        ),
        (
            YamlValue::String("sinks".to_owned()),
            YamlValue::Mapping(sinks),
        ),
    ]))))
}

fn validation_source(component: &Value) -> Option<YamlValue> {
    let input = component.get("input").and_then(Value::as_object);
    if input.is_none_or(|input| json_truthy(input.get("logs"))) {
        return Some(YamlValue::Mapping(Mapping::from_iter([
            (
                YamlValue::String("type".to_owned()),
                YamlValue::String("demo_logs".to_owned()),
            ),
            (
                YamlValue::String("format".to_owned()),
                YamlValue::String("json".to_owned()),
            ),
        ])));
    }
    input
        .filter(|input| json_truthy(input.get("metrics")))
        .map(|_| {
            YamlValue::Mapping(Mapping::from_iter([(
                YamlValue::String("type".to_owned()),
                YamlValue::String("internal_metrics".to_owned()),
            )]))
        })
}

fn json_truthy(value: Option<&Value>) -> bool {
    value.is_some_and(|value| match value {
        Value::Null => false,
        Value::Bool(value) => *value,
        Value::Number(value) => value.as_f64().is_none_or(|value| value != 0.0),
        Value::String(value) => !value.is_empty(),
        Value::Array(_) | Value::Object(_) => true,
    })
}

fn route_outputs(transform: &Mapping) -> Vec<String> {
    let mut outputs = BTreeSet::new();
    if let Some(route) = mapping_value(transform, "route").and_then(YamlValue::as_mapping) {
        outputs.extend(
            route
                .keys()
                .filter_map(YamlValue::as_str)
                .map(str::to_owned),
        );
    }
    if let Some(routes) = mapping_value(transform, "routes").and_then(YamlValue::as_sequence) {
        outputs.extend(routes.iter().filter_map(|route| {
            route
                .as_mapping()
                .and_then(|route| mapping_value(route, "name"))
                .and_then(YamlValue::as_str)
                .map(str::to_owned)
        }));
    }
    outputs.into_iter().collect()
}

fn blackhole(input: &str) -> YamlValue {
    YamlValue::Mapping(Mapping::from_iter([
        (
            YamlValue::String("type".to_owned()),
            YamlValue::String("blackhole".to_owned()),
        ),
        (YamlValue::String("inputs".to_owned()), yaml_list([input])),
    ]))
}

fn yaml_list<'a>(values: impl IntoIterator<Item = &'a str>) -> YamlValue {
    YamlValue::Sequence(
        values
            .into_iter()
            .map(|value| YamlValue::String(value.to_owned()))
            .collect(),
    )
}

fn yaml_mapping(value: &YamlValue, description: &str) -> Result<Mapping> {
    value
        .as_mapping()
        .cloned()
        .ok_or_else(|| anyhow!("Expected {description} to be a YAML mapping"))
}

fn mapping_value<'a>(mapping: &'a Mapping, key: &str) -> Option<&'a YamlValue> {
    mapping.get(YamlValue::String(key.to_owned()))
}

fn mapping_key(mapping: &Mapping, description: &str) -> Result<String> {
    mapping
        .keys()
        .find_map(YamlValue::as_str)
        .map(str::to_owned)
        .ok_or_else(|| anyhow!("{description} has no component identifier"))
}

fn vector_binary(repo_root: &Path) -> Result<PathBuf> {
    if let Some(vector_bin) = std::env::var_os("VECTOR_BIN") {
        return Ok(PathBuf::from(vector_bin));
    }

    let status = Command::new("cargo")
        .current_dir(repo_root)
        .args(["build", "--bin", "vector"])
        .status()
        .context("Failed to build Vector for example validation")?;
    if !status.success() {
        bail!("Failed to build Vector for example validation")
    }

    let target_dir = std::env::var_os("CARGO_TARGET_DIR").map_or_else(
        || repo_root.join("target"),
        |path| {
            let path = PathBuf::from(path);
            if path.is_absolute() {
                path
            } else {
                repo_root.join(path)
            }
        },
    );
    let vector_bin = target_dir
        .join("debug")
        .join(format!("vector{}", std::env::consts::EXE_SUFFIX));
    if !vector_bin.is_file() {
        bail!(
            "Vector build completed but the binary was not found at {}",
            vector_bin.display()
        )
    }
    Ok(vector_bin)
}

fn validate_case(repo_root: &Path, vector_bin: &Path, case: &ValidationCase) -> Result<()> {
    let temporary = Builder::new()
        .prefix("vector-validate-example-")
        .suffix(".yaml")
        .tempfile()
        .context("Failed to create a temporary validation file")?;
    serde_yaml::to_writer(temporary.as_file(), &case.config)
        .with_context(|| format!("Failed to serialize {}", case.key))?;

    let output = Command::new(vector_bin)
        .current_dir(repo_root)
        .args(["validate", "--no-environment", "--skip-healthchecks"])
        .arg(temporary.path())
        .output()
        .with_context(|| format!("Failed to validate {}", case.key))?;

    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let details = if stderr.trim().is_empty() {
        &stdout
    } else {
        &stderr
    };
    bail!("{}", summarize_error(details))
}

fn summarize_error(error: &str) -> &str {
    error
        .lines()
        .find(|line| {
            let line = line.trim();
            !line.contains("Failed to load") && !line.starts_with("error[") && line.contains("x ")
        })
        .or_else(|| error.lines().find(|line| !line.trim().is_empty()))
        .unwrap_or(error)
        .trim()
        .trim_start_matches("x ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn component_example_changes_are_detected() {
        let changes = [
            "website/generated/example-configs/sources/file/minimal.yaml",
            "website/generated/example-configs/transforms/lua/advanced.yaml",
            "website/cue/reference/components/sources/file.cue",
        ];

        let examples = component_example_changes(changes.into_iter().map(str::to_owned).collect());

        assert_eq!(
            examples,
            [
                "website/generated/example-configs/sources/file/minimal.yaml",
                "website/generated/example-configs/transforms/lua/advanced.yaml",
            ]
        );
    }
}
