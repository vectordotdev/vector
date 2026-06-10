use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};

use super::{Format, component_name, interpolate_toml_table_with_env_vars, open_file, read_dir};
use crate::config::loading::schema_coercion::coerce;
use crate::config::{ConfigBuilder, format};
use serde_toml_merge::merge_into_table;
use toml::value::{Table, Value};
use vector_config::schema::generate_root_schema;

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
    use std::io::Read;

    /// This trait contains methods that deserialize files/folders. There are a few methods
    /// in here with subtly different names that can be hidden from public view, hence why
    /// this is nested in a private mod.
    pub trait Process {
        /// This is invoked after input deserialization. This can be a useful step to interpolate
        /// environment variables or perform some other post-processing on the table.
        fn postprocess(&mut self, table: Table) -> Result<Table, Vec<String>>;

        /// Returns whether environment variable interpolation should be applied.
        /// Default is true; override to disable.
        fn should_interpolate_env(&self) -> bool {
            true
        }

        /// Deserializes the input using the given format and runs postprocessing on the result.
        ///
        /// This reads the input into a string, deserializes it into a `Table`, and then
        /// applies `postprocess` to the resulting table.
        fn load<R: Read>(&mut self, input: R, format: Format) -> Result<Table, Vec<String>> {
            let value = string_from_input(input)?;
            let table: Table = format::deserialize(&value, format)
                .map_err(|errs| annotate_unquoted_placeholders(errs, &value, format))?;
            let table = if self.should_interpolate_env() {
                resolve_environment_variables(table)?
            } else {
                table
            };
            self.postprocess(table)
        }

        /// Helper method used by other methods to recursively handle file/dir loading, merging
        /// values against a provided TOML `Table`.
        fn load_dir_into(
            &mut self,
            path: &Path,
            result: &mut Table,
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
                            "Could not read entry in config dir: {:?}, {}.",
                            path, err
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
                        if let Err(errs) = merge_with_value(result, name, Value::Table(inner)) {
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
                            Ok(table) => {
                                result.insert(name, Value::Table(table));
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

        /// Loads and deserializes a file into a TOML `Table`.
        fn load_file(
            &mut self,
            path: &Path,
            format: Format,
        ) -> Result<Option<(String, Table)>, Vec<String>> {
            if let (Ok(name), Some(file)) = (component_name(path), open_file(path)) {
                self.load(file, format).map(|value| Some((name, value)))
            } else {
                Ok(None)
            }
        }

        /// Loads a file, and if the path provided contains a sub-folder by the same name as the
        /// component, descend into it recursively, returning a TOML `Table`.
        fn load_file_recursive(
            &mut self,
            path: &Path,
            format: Format,
        ) -> Result<Option<(String, Table)>, Vec<String>> {
            if let Some((name, mut table)) = self.load_file(path, format)? {
                if let Some(subdir) = path.parent().map(|p| p.join(&name))
                    && subdir.is_dir()
                    && subdir.exists()
                {
                    self.load_dir_into(&subdir, &mut table, true)?;
                }
                Ok(Some((name, table)))
            } else {
                Ok(None)
            }
        }

        /// Loads a directory (optionally, recursively), returning a TOML `Table`. This will
        /// create an initial `Table` and pass it into `load_dir_into` for recursion handling.
        fn load_dir(&mut self, path: &Path, recurse: bool) -> Result<Table, Vec<String>> {
            let mut result = Table::new();
            self.load_dir_into(path, &mut result, recurse)?;
            Ok(result)
        }

        /// Merge a provided TOML `Table` in an implementation-specific way. Contains an
        /// optional component hint, which may affect how components are merged. Takes a `&mut self`
        /// with the intention of merging an inner value that can be `take`n by a `Loader`.
        fn merge(&mut self, table: Table, hint: Option<ComponentHint>) -> Result<(), Vec<String>>;
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

    /// Deserializes a file with the provided format, and makes the result available via `take`.
    /// Returns a vector of non-fatal warnings on success, or a vector of error strings on failure.
    fn load_from_file(&mut self, path: &Path, format: Format) -> Result<(), Vec<String>> {
        if let Some((_, table)) = self.load_file(path, format)? {
            self.merge(table, None)?;
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
        let mut root = Table::new();
        let table = self.load_dir(path, false)?;

        // Discard the named part of the path, since these don't form any component names.
        for (_, value) in table {
            // All files should contain key/value pairs.
            if let Value::Table(table) = value {
                merge_into_table(&mut root, table).map_err(|e| vec![e.to_string()])?;
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
                let table = self.load_dir(&path, matches!(hint, ComponentHint::Transform))?;

                self.merge(table, Some(hint))?;
            }
        }

        Ok(())
    }
}

/// Merge two TOML `Value`s, returning a new `Value`.
fn merge_values(value: toml::Value, other: toml::Value) -> Result<toml::Value, Vec<String>> {
    serde_toml_merge::merge(value, other).map_err(|e| vec![e.to_string()])
}

/// Updates a TOML `Table` with the merged values of a named key. Inserts if it doesn't exist.
fn merge_with_value(res: &mut Table, name: String, value: toml::Value) -> Result<(), Vec<String>> {
    if let Some(existing) = res.remove(&name) {
        res.insert(name, merge_values(existing, value)?);
    } else {
        res.insert(name, value);
    }
    Ok(())
}

/// Deserialize a TOML `Table` into a `T`, walking the root `ConfigBuilder`
/// JSON Schema to coerce string scalars to their declared types and to detect
/// unknown fields with full field paths.
pub(super) fn deserialize_table<T: serde::de::DeserializeOwned>(
    table: Table,
) -> Result<T, Vec<String>> {
    deserialize_table_inner(table, None)
}

/// Deserialize a hinted component sub-table (e.g. the `sources` map from a
/// namespaced directory) against the root schema by temporarily wrapping it
/// under `wrapper_key`, then extracting the inner value after coercion. This
/// gives the namespaced path the same schema coverage as the inline form.
pub(super) fn deserialize_table_wrapped<T: serde::de::DeserializeOwned>(
    table: Table,
    wrapper_key: &str,
) -> Result<T, Vec<String>> {
    deserialize_table_inner(table, Some(wrapper_key))
}

fn deserialize_table_inner<T: serde::de::DeserializeOwned>(
    table: Table,
    wrapper_key: Option<&str>,
) -> Result<T, Vec<String>> {
    let inner_json = serde_json::to_value(table)
        .map_err(|err| err.to_string())
        .map_err(|err| vec![err])?;

    let mut table_json = match wrapper_key {
        Some(key) => serde_json::json!({ key: inner_json }),
        None => inner_json,
    };

    let schema = generate_root_schema::<ConfigBuilder>().map_err(|e| vec![format!("{e:?}")])?;
    let schema_json = serde_json::to_value(schema).map_err(|err| vec![err.to_string()])?;
    coerce(
        &mut table_json,
        &schema_json,
        schema_json.get("definitions"),
        &mut Vec::new(),
    )
    .map_err(|err| vec![err.to_string()])?;

    let to_deserialize = match wrapper_key {
        Some(key) => table_json
            .as_object_mut()
            .and_then(|m| m.remove(key))
            .ok_or_else(|| vec![format!("internal: missing wrapper key '{key}'")])?,
        None => table_json,
    };

    serde::Deserialize::deserialize(to_deserialize).map_err(|err| vec![err.to_string()])
}

fn string_from_input<R: Read>(mut input: R) -> Result<String, Vec<String>> {
    let mut source_string = String::new();
    input
        .read_to_string(&mut source_string)
        .map_err(|e| vec![e.to_string()])?;
    Ok(source_string)
}

pub fn load<R: Read, T>(input: R, format: Format, interpolate_env: bool) -> Result<T, Vec<String>>
where
    T: serde::de::DeserializeOwned,
{
    let value = string_from_input(input)?;
    let table = format::deserialize(&value, format)?;
    let table = if interpolate_env {
        resolve_environment_variables(table)?
    } else {
        table
    };
    deserialize_table(table)
}

pub fn resolve_environment_variables(table: Table) -> Result<Table, Vec<String>> {
    let mut vars = std::env::vars().collect::<HashMap<_, _>>();
    if !vars.contains_key("HOSTNAME")
        && let Ok(hostname) = crate::get_hostname()
    {
        vars.insert("HOSTNAME".into(), hostname);
    }

    interpolate_toml_table_with_env_vars(&table, &vars)
}

/// If a parse error came from a TOML or JSON config that contains an unquoted
/// `${VAR}` or `SECRET[...]` placeholder, prepend a hint to the error explaining
/// the migration. Configs of this shape worked under the pre-parse interpolation
/// pipeline, but they are not valid TOML/JSON syntax and now fail at parse time.
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

/// Scan `source` for the first occurrence of `${...}` or `SECRET[...]` that is not
/// immediately surrounded by quote characters on the same line. Returns the
/// 1-based line number, the line text (trimmed of trailing newline), and the matched
/// placeholder text.
fn find_unquoted_placeholder(source: &str) -> Option<(usize, &str, String)> {
    for (idx, line) in source.lines().enumerate() {
        if let Some(p) = scan_line_for_unquoted_placeholder(line) {
            return Some((idx + 1, line, p));
        }
    }
    None
}

fn scan_line_for_unquoted_placeholder(line: &str) -> Option<String> {
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        // ${...}
        if bytes[i] == b'$'
            && i + 1 < bytes.len()
            && bytes[i + 1] == b'{'
            && let Some(end) = line[i + 2..].find('}')
        {
            let placeholder_end = i + 2 + end + 1;
            let placeholder = &line[i..placeholder_end];
            if !is_wrapped_in_quotes(line, i, placeholder_end) {
                return Some(placeholder.to_string());
            }
            i = placeholder_end;
            continue;
        }

        // SECRET[...]
        if line[i..].starts_with("SECRET[")
            && let Some(end) = line[i + 7..].find(']')
        {
            let placeholder_end = i + 7 + end + 1;
            let placeholder = &line[i..placeholder_end];
            if !is_wrapped_in_quotes(line, i, placeholder_end) {
                return Some(placeholder.to_string());
            }
            i = placeholder_end;
            continue;
        }

        i += 1;
    }
    None
}

fn is_wrapped_in_quotes(line: &str, start: usize, end: usize) -> bool {
    let bytes = line.as_bytes();
    let prev = start.checked_sub(1).map(|p| bytes[p]);
    let next = bytes.get(end).copied();
    matches!(prev, Some(b'"') | Some(b'\'')) && matches!(next, Some(b'"') | Some(b'\''))
}

#[cfg(test)]
mod placeholder_hint_tests {
    use super::{Format, annotate_unquoted_placeholders, find_unquoted_placeholder};

    #[test]
    fn finds_unquoted_env_var_in_toml() {
        let src = "[sources.in]\ntype = \"demo_logs\"\ncount = ${MY_COUNT}\n";
        let (line, _, placeholder) = find_unquoted_placeholder(src).expect("should detect");
        assert_eq!(line, 3);
        assert_eq!(placeholder, "${MY_COUNT}");
    }

    #[test]
    fn ignores_quoted_env_var() {
        let src = "[sources.in]\ntype = \"demo_logs\"\ncount = \"${MY_COUNT}\"\n";
        assert!(find_unquoted_placeholder(src).is_none());
    }

    #[test]
    fn finds_unquoted_secret_in_json() {
        let src = "{\"port\": SECRET[vault.port]}\n";
        let (_, _, placeholder) = find_unquoted_placeholder(src).expect("should detect");
        assert_eq!(placeholder, "SECRET[vault.port]");
    }

    #[test]
    fn ignores_secret_inside_string_value() {
        let src = "{\"key\": \"SECRET[vault.api_key]\"}\n";
        assert!(find_unquoted_placeholder(src).is_none());
    }

    #[test]
    fn annotation_only_applied_to_toml_or_json() {
        let errs = vec!["some parse error".to_string()];
        // YAML is unaffected even if the source happens to contain a bare ${VAR}.
        let yaml_src = "count: ${MY_COUNT}\n";
        let yaml = annotate_unquoted_placeholders(errs.clone(), yaml_src, Format::Yaml);
        assert_eq!(yaml, errs);

        // TOML gets the hint prepended.
        let toml_src = "count = ${MY_COUNT}\n";
        let toml = annotate_unquoted_placeholders(errs.clone(), toml_src, Format::Toml);
        assert_eq!(toml.len(), errs.len() + 1);
        assert!(toml[0].contains("Wrap the placeholder in quotes"));
    }
}
