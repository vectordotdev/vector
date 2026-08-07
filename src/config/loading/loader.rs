use std::{
    collections::HashMap,
    io::Read,
    path::{Path, PathBuf},
};

use serde_json::Value;
use vector_config::schema::generate_root_schema;

use super::{
    Format, component_name, interpolate_config_map_with_env_vars, open_file, read_dir,
    representation::{
        ConfigMap, deserialize_config_value, merge_into_map, merge_values, parse_config_value,
    },
    schema_coercion::coerce,
};
use crate::config::ConfigBuilder;

/// Provides a hint to the loading system of the type of components that should be found
/// when traversing an explicitly named directory.
#[derive(Debug, Copy, Clone)]
pub enum ComponentHint {
    Source,
    Transform,
    Sink,
    Test,
    EnrichmentTable,
}

impl ComponentHint {
    /// Returns the component string field that should host a component -- e.g. sources,
    /// transforms, etc.
    const fn as_component_field(&self) -> &str {
        match self {
            ComponentHint::Source => "sources",
            ComponentHint::Transform => "transforms",
            ComponentHint::Sink => "sinks",
            ComponentHint::Test => "tests",
            ComponentHint::EnrichmentTable => "enrichment_tables",
        }
    }

    /// Joins a component sub-folder to a provided path, for traversal. Since `Self` is a
    /// `Copy`, this is more efficient to pass by value than ref.
    pub fn join_path(self, path: &Path) -> PathBuf {
        path.join(self.as_component_field())
    }
}

// The loader traits are split into two parts -- an internal `process` mod, that contains
// functionality for processing files/folders, and a `Loader<T>` trait, that provides a public
// interface getting a `T` from a file/folder. The private mod is available to implementors
// within the loading mod, but does not form part of the public interface. This is useful
// because there are numerous internal functions for dealing with (non)recursive loading that
// rely on `&self` but don't need overriding and would be confusingly named in a public API.
pub(super) mod process {
    use super::*;

    /// This trait contains methods that deserialize files/folders. There are a few methods
    /// in here with subtly different names that can be hidden from public view, hence why
    /// this is nested in a private mod.
    pub trait Process {
        /// Runs implementation-specific processing after parsing and interpolation.
        fn postprocess(&mut self, map: ConfigMap) -> Result<ConfigMap, Vec<String>>;

        /// Returns whether environment variable interpolation should be applied.
        fn should_interpolate_env(&self) -> bool {
            true
        }

        /// Parses input, interpolates string leaves, and runs implementation-specific processing.
        fn load<R: Read>(&mut self, input: R, format: Format) -> Result<ConfigMap, Vec<String>> {
            let source = string_from_input(input)?;
            let value = parse_config_value(&source, format)
                .map_err(|errors| annotate_unquoted_placeholders(errors, &source, format))?;
            let map = deserialize_config_value(value)?;
            let map = if self.should_interpolate_env() {
                resolve_environment_variables(map)?
            } else {
                map
            };

            self.postprocess(map)
        }

        /// Helper method used by other methods to recursively handle file/dir loading, merging
        /// values against a provided configuration map.
        fn load_dir_into(
            &mut self,
            path: &Path,
            result: &mut ConfigMap,
            recurse: bool,
        ) -> Result<(), Vec<String>> {
            let mut errors = Vec::new();
            let readdir = read_dir(path)?;

            let mut files = Vec::new();
            let mut folders = Vec::new();

            for entry in readdir {
                match entry {
                    Ok(item) => {
                        let entry = item.path();
                        if entry.is_file() {
                            files.push(entry);
                        } else if entry.is_dir() {
                            // do not load directories when the directory starts with a '.'
                            if !entry
                                .file_name()
                                .and_then(|name| name.to_str())
                                .map(|name| name.starts_with('.'))
                                .unwrap_or(false)
                            {
                                folders.push(entry);
                            }
                        }
                    }
                    Err(err) => {
                        errors.push(format!(
                            "Could not read entry in config dir: {path:?}, {err}."
                        ));
                    }
                };
            }

            for entry in files {
                // If the file doesn't contain a known extension, skip it.
                let format = match Format::from_path(&entry) {
                    Ok(format) => format,
                    _ => continue,
                };

                let loaded = if recurse {
                    self.load_file_recursive(&entry, format)
                } else {
                    self.load_file(&entry, format)
                };

                match loaded {
                    Ok(Some((name, inner))) => {
                        if let Err(errs) = merge_with_value(result, name, Value::Object(inner)) {
                            errors.extend(errs);
                        }
                    }
                    Ok(None) => {}
                    Err(errs) => {
                        errors.extend(errs);
                    }
                }
            }

            // Only descend into folders if `recurse: true`.
            if recurse {
                for entry in folders {
                    if let Ok(name) = component_name(&entry)
                        && !result.contains_key(&name)
                    {
                        match self.load_dir(&entry, true) {
                            Ok(map) => {
                                result.insert(name, Value::Object(map));
                            }
                            Err(errs) => {
                                errors.extend(errs);
                            }
                        }
                    }
                }
            }

            if errors.is_empty() {
                Ok(())
            } else {
                Err(errors)
            }
        }

        /// Loads and deserializes a file into a configuration map.
        fn load_file(
            &mut self,
            path: &Path,
            format: Format,
        ) -> Result<Option<(String, ConfigMap)>, Vec<String>> {
            match (component_name(path), open_file(path)) {
                (Ok(name), Some(file)) => self.load(file, format).map(|value| Some((name, value))),
                _ => Ok(None),
            }
        }

        /// Loads a file, and if the path provided contains a sub-folder by the same name as the
        /// component, descend into it recursively, returning a configuration map.
        fn load_file_recursive(
            &mut self,
            path: &Path,
            format: Format,
        ) -> Result<Option<(String, ConfigMap)>, Vec<String>> {
            if let Some((name, mut map)) = self.load_file(path, format)? {
                if let Some(subdir) = path.parent().map(|p| p.join(&name))
                    && subdir.is_dir()
                    && subdir.exists()
                {
                    self.load_dir_into(&subdir, &mut map, true)?;
                }
                Ok(Some((name, map)))
            } else {
                Ok(None)
            }
        }

        /// Loads a directory (optionally, recursively), returning a configuration map. This will
        /// create an initial map and pass it into `load_dir_into` for recursion handling.
        fn load_dir(&mut self, path: &Path, recurse: bool) -> Result<ConfigMap, Vec<String>> {
            let mut result = ConfigMap::new();
            self.load_dir_into(path, &mut result, recurse)?;
            Ok(result)
        }

        /// Merge a provided configuration map in an implementation-specific way. Contains an
        /// optional component hint, which may affect how components are merged. Takes a `&mut self`
        /// with the intention of merging an inner value that can be `take`n by a `Loader`.
        fn merge(&mut self, map: ConfigMap, hint: Option<ComponentHint>)
        -> Result<(), Vec<String>>;
    }
}

/// `Loader` represents the public part of the loading interface. Includes methods for loading
/// from a file or folder, and accessing the final deserialized `T` value via the `take` method.
pub trait Loader<T>: process::Process
where
    T: serde::de::DeserializeOwned,
{
    /// Consumes Self, and returns the final, deserialized `T`.
    fn take(self) -> T;

    fn load_from_str<R: std::io::Read>(
        &mut self,
        input: R,
        format: Format,
    ) -> Result<(), Vec<String>> {
        let map = self.load(input, format)?;
        self.merge(map, None)
    }

    /// Deserializes a file with the provided format, and makes the result available via `take`.
    /// Returns a vector of non-fatal warnings on success, or a vector of error strings on failure.
    fn load_from_file(&mut self, path: &Path, format: Format) -> Result<(), Vec<String>> {
        if let Some((_, map)) = self.load_file(path, format)? {
            self.merge(map, None)?;
            Ok(())
        } else {
            Ok(())
        }
    }

    /// Deserializes a dir with the provided format, and makes the result available via `take`.
    /// Returns a vector of non-fatal warnings on success, or a vector of error strings on failure.
    fn load_from_dir(&mut self, path: &Path) -> Result<(), Vec<String>> {
        // Iterator containing component-specific sub-folders to attempt traversing into.
        let hints = [
            ComponentHint::Source,
            ComponentHint::Transform,
            ComponentHint::Sink,
            ComponentHint::Test,
            ComponentHint::EnrichmentTable,
        ];
        let paths = hints
            .iter()
            .copied()
            .map(|hint| (hint.join_path(path), hint));

        // Get files from the root of the folder. These represent top-level config settings,
        // and need to merged down first to represent a more 'complete' config.
        let mut root = ConfigMap::new();
        let map = self.load_dir(path, false)?;

        // Discard the named part of the path, since these don't form any component names.
        for (_, value) in map {
            // All files should contain key/value pairs.
            if let Value::Object(map) = value {
                merge_into_map(&mut root, map)?;
            }
        }

        // Merge the 'root' config value first.
        self.merge(root, None)?;

        // Loop over each component path. If it exists, load files and merge.
        for (path, hint) in paths {
            // Sanity check for paths, to ensure we're dealing with a folder. This is necessary
            // because a sub-folder won't generally exist unless the config is namespaced.
            if path.exists() && path.is_dir() {
                // Transforms are treated differently from other component types; they can be
                // arbitrarily nested.
                let map = self.load_dir(&path, matches!(hint, ComponentHint::Transform))?;

                self.merge(map, Some(hint))?;
            }
        }

        Ok(())
    }
}

/// Updates a configuration map with the merged values of a named key. Inserts if absent.
fn merge_with_value(res: &mut ConfigMap, name: String, value: Value) -> Result<(), Vec<String>> {
    if let Some(existing) = res.remove(&name) {
        res.insert(name, merge_values(existing, value)?);
    } else {
        res.insert(name, value);
    }
    Ok(())
}

/// Deserialize a configuration map into a `T`, coercing string scalars according
/// to the root `ConfigBuilder` JSON Schema.
pub(super) fn deserialize_config_map<T: serde::de::DeserializeOwned>(
    map: ConfigMap,
) -> Result<T, Vec<String>> {
    deserialize_config_map_inner(map, None)
}

/// Deserialize a namespaced component map against its corresponding root field.
pub(super) fn deserialize_config_map_wrapped<T: serde::de::DeserializeOwned>(
    map: ConfigMap,
    wrapper_key: &str,
) -> Result<T, Vec<String>> {
    deserialize_config_map_inner(map, Some(wrapper_key))
}

fn deserialize_config_map_inner<T: serde::de::DeserializeOwned>(
    map: ConfigMap,
    wrapper_key: Option<&str>,
) -> Result<T, Vec<String>> {
    let inner = Value::Object(map);
    let mut config = match wrapper_key {
        Some(key) => serde_json::json!({ key: inner }),
        None => inner,
    };

    let schema =
        generate_root_schema::<ConfigBuilder>().map_err(|error| vec![format!("{error:?}")])?;
    let schema = serde_json::to_value(schema).map_err(|error| vec![error.to_string()])?;
    coerce(
        &mut config,
        &schema,
        schema.get("definitions"),
        &mut Vec::new(),
    )
    .map_err(|error| vec![error.to_string()])?;

    let value = match wrapper_key {
        Some(key) => config
            .as_object_mut()
            .and_then(|map| map.remove(key))
            .ok_or_else(|| vec![format!("internal: missing wrapper key '{key}'")])?,
        None => config,
    };

    deserialize_config_value(value)
}

fn string_from_input<R: Read>(mut input: R) -> Result<String, Vec<String>> {
    let mut source = String::new();
    input
        .read_to_string(&mut source)
        .map_err(|error| vec![error.to_string()])?;
    Ok(source)
}

pub fn load<R: Read, T>(input: R, format: Format) -> Result<T, Vec<String>>
where
    T: serde::de::DeserializeOwned,
{
    let source = string_from_input(input)?;
    let value = parse_config_value(&source, format)?;
    let map = deserialize_config_value(value)?;
    deserialize_config_map(map)
}

pub fn resolve_environment_variables(map: ConfigMap) -> Result<ConfigMap, Vec<String>> {
    let mut vars = std::env::vars_os()
        .filter_map(|(key, value)| Some((key.into_string().ok()?, value.into_string().ok()?)))
        .collect::<HashMap<_, _>>();

    if !vars.contains_key("HOSTNAME")
        && let Ok(hostname) = crate::get_hostname()
    {
        vars.insert("HOSTNAME".into(), hostname);
    }

    interpolate_config_map_with_env_vars(&map, &vars)
}

/// Adds a migration hint when TOML or JSON contains an unquoted placeholder.
fn annotate_unquoted_placeholders(
    errors: Vec<String>,
    source: &str,
    format: Format,
) -> Vec<String> {
    if !matches!(format, Format::Toml | Format::Json) {
        return errors;
    }

    let Some((line_no, line, placeholder)) = find_unquoted_placeholder(source) else {
        return errors;
    };

    let hint = format!(
        "Config contains an unquoted placeholder `{placeholder}` at line {line_no}:\n  \
         {line}\n\
         Wrap the placeholder in quotes so it parses as a string. Vector will coerce \
         the value to the declared field type at load time.\n  \
         Example: `field = \"{placeholder}\"`"
    );

    let mut annotated = Vec::with_capacity(errors.len() + 1);
    annotated.push(hint);
    annotated.extend(errors);
    annotated
}

fn find_unquoted_placeholder(source: &str) -> Option<(usize, &str, String)> {
    for (index, line) in source.lines().enumerate() {
        if let Some(placeholder) = scan_line_for_unquoted_placeholder(line) {
            return Some((index + 1, line, placeholder));
        }
    }
    None
}

fn scan_line_for_unquoted_placeholder(line: &str) -> Option<String> {
    let bytes = line.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'$'
            && index + 1 < bytes.len()
            && bytes[index + 1] == b'{'
            && let Some(relative_end) = bytes[index + 2..].iter().position(|&byte| byte == b'}')
        {
            let end = index + 2 + relative_end + 1;
            let placeholder =
                std::str::from_utf8(&bytes[index..end]).expect("bounds are at ASCII chars");
            if !is_wrapped_in_quotes(line, index, end) {
                return Some(placeholder.to_string());
            }
            index = end;
            continue;
        }

        if bytes[index..].starts_with(b"SECRET[")
            && let Some(relative_end) = bytes[index + 7..].iter().position(|&byte| byte == b']')
        {
            let end = index + 7 + relative_end + 1;
            let placeholder =
                std::str::from_utf8(&bytes[index..end]).expect("bounds are at ASCII chars");
            if !is_wrapped_in_quotes(line, index, end) {
                return Some(placeholder.to_string());
            }
            index = end;
            continue;
        }

        index += 1;
    }
    None
}

fn is_wrapped_in_quotes(line: &str, start: usize, end: usize) -> bool {
    let bytes = line.as_bytes();
    let previous = start.checked_sub(1).map(|position| bytes[position]);
    let next = bytes.get(end).copied();
    matches!(previous, Some(b'"') | Some(b'\'')) && matches!(next, Some(b'"') | Some(b'\''))
}

#[cfg(test)]
mod placeholder_hint_tests {
    use super::{Format, annotate_unquoted_placeholders, find_unquoted_placeholder};

    #[test]
    fn finds_unquoted_env_var_in_toml() {
        let source = "[sources.in]\ntype = \"demo_logs\"\ncount = ${MY_COUNT}\n";
        let (line, _, placeholder) = find_unquoted_placeholder(source).expect("should detect");
        assert_eq!(line, 3);
        assert_eq!(placeholder, "${MY_COUNT}");
    }

    #[test]
    fn ignores_quoted_env_var() {
        let source = "[sources.in]\ntype = \"demo_logs\"\ncount = \"${MY_COUNT}\"\n";
        assert!(find_unquoted_placeholder(source).is_none());
    }

    #[test]
    fn finds_unquoted_secret_in_json() {
        let source = "{\"port\": SECRET[vault.port]}\n";
        let (_, _, placeholder) = find_unquoted_placeholder(source).expect("should detect");
        assert_eq!(placeholder, "SECRET[vault.port]");
    }

    #[test]
    fn ignores_secret_inside_string_value() {
        let source = "{\"key\": \"SECRET[vault.api_key]\"}\n";
        assert!(find_unquoted_placeholder(source).is_none());
    }

    #[test]
    fn annotation_only_applied_to_toml_or_json() {
        let errors = vec!["some parse error".to_string()];
        let yaml =
            annotate_unquoted_placeholders(errors.clone(), "count: ${MY_COUNT}\n", Format::Yaml);
        assert_eq!(yaml, errors);

        let toml =
            annotate_unquoted_placeholders(errors.clone(), "count = ${MY_COUNT}\n", Format::Toml);
        assert_eq!(toml.len(), errors.len() + 1);
        assert!(toml[0].contains("Wrap the placeholder in quotes"));
    }
}
