use std::path::{Path, PathBuf};

use serde_toml_merge::merge_into_table;
use toml::value::{Table, Value};

use super::{component_name, open_file, read_dir, Format};
use crate::config::format;

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
    use std::io::Read;

    use super::*;

    /// This trait contains methods that deserialize files/folders. There are a few methods
    /// in here with subtly different names that can be hidden from public view, hence why
    /// this is nested in a private mod.
    pub trait Process {
        /// Prepares input for serialization. This can be a useful step to interpolate
        /// environment variables or perform some other pre-processing on the input.
        fn prepare<R: Read>(&mut self, input: R) -> Result<(String, Vec<String>), Vec<String>>;

        /// Calls into the `prepare` method, and deserializes a `Read` to a `T`.
        fn load<R: std::io::Read, T>(
            &mut self,
            input: R,
            format: Format,
        ) -> Result<(T, Vec<String>), Vec<String>>
        where
            T: serde::de::DeserializeOwned,
        {
            let (value, warnings) = self.prepare(input)?;

            format::deserialize(&value, format).map(|builder| (builder, warnings))
        }

        /// Helper method used by other methods to recursively handle file/dir loading, merging
        /// values against a provided TOML `Table`.
        fn load_dir_into(
            &mut self,
            path: &Path,
            result: &mut Table,
            recurse: bool,
        ) -> Result<Vec<String>, Vec<String>> {
            let mut errors = Vec::new();
            let mut warnings = Vec::new();
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
                    Ok(Some((name, inner, warns))) => {
                        if let Err(errs) = merge_with_value(result, name, Value::Table(inner)) {
                            errors.extend(errs);
                        } else {
                            warnings.extend(warns);
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
                    if let Ok(name) = component_name(&entry) {
                        if !result.contains_key(&name) {
                            match self.load_dir(&entry, true) {
                                Ok((table, warns)) => {
                                    result.insert(name, Value::Table(table));
                                    warnings.extend(warns);
                                }
                                Err(errs) => {
                                    errors.extend(errs);
                                }
                            }
                        }
                    }
                }
            }

            if errors.is_empty() {
                Ok(warnings)
            } else {
                Err(errors)
            }
        }

        /// Loads and deserializes a file into a TOML `Table`.
        fn load_file(
            &mut self,
            path: &Path,
            format: Format,
        ) -> Result<Option<(String, Table, Vec<String>)>, Vec<String>> {
            if let (Ok(name), Some(file)) = (component_name(path), open_file(path)) {
                self.load(file, format)
                    .map(|(value, warnings)| Some((name, value, warnings)))
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
        ) -> Result<Option<(String, Table, Vec<String>)>, Vec<String>> {
            if let Some((name, mut table, mut warnings)) = self.load_file(path, format)? {
                if let Some(subdir) = path.parent().map(|p| p.join(&name)) {
                    if subdir.is_dir() && subdir.exists() {
                        warnings.extend(self.load_dir_into(&subdir, &mut table, true)?);
                    }
                }
                Ok(Some((name, table, warnings)))
            } else {
                Ok(None)
            }
        }

        /// Loads a directory (optionally, recursively), returning a TOML `Table`. This will
        /// create an initial `Table` and pass it into `load_dir_into` for recursion handling.
        fn load_dir(
            &mut self,
            path: &Path,
            recurse: bool,
        ) -> Result<(Table, Vec<String>), Vec<String>> {
            let mut result = Table::new();
            let warnings = self.load_dir_into(path, &mut result, recurse)?;
            Ok((result, warnings))
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
    fn load_from_file(&mut self, path: &Path, format: Format) -> Result<Vec<String>, Vec<String>> {
        if let Some((_, table, warnings)) = self.load_file(path, format)? {
            self.merge(table, None)?;
            Ok(warnings)
        } else {
            Ok(vec![])
        }
    }

    /// Deserializes a dir with the provided format, and makes the result available via `take`.
    /// Returns a vector of non-fatal warnings on success, or a vector of error strings on failure.
    fn load_from_dir(&mut self, path: &Path) -> Result<Vec<String>, Vec<String>> {
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
        let (table, mut warnings) = self.load_dir(path, false)?;

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
                let (table, warns) =
                    self.load_dir(&path, matches!(hint, ComponentHint::Transform))?;

                self.merge(table, Some(hint))?;

                warnings.extend(warns);
            }
        }

        Ok(warnings)
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

/// Deserialize a TOML `Table` into a `T`.
pub(super) fn deserialize_table<T: serde::de::DeserializeOwned>(
    table: Table,
) -> Result<T, Vec<String>> {
    Value::Table(table)
        .try_into()
        .map_err(|e| vec![e.to_string()])
}
