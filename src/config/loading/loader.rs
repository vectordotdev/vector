use super::{component_name, open_file, read_dir, Format};
use crate::config::format;
use serde_toml_merge::merge_into_table;
use std::path::Path;
use toml::value::{Table, Value};

// The loader traits are split into two parts -- an internal `process` mod, that contains
// functionality for deserializing a type `T`, and a `Loader` trait, that provides a public interface
// getting a `T` from a file/directory. The private mod is available to implementors within
// the loading mod, but does not form part of the public interface.
pub(super) mod process {
    use super::*;
    use std::fmt::Debug;
    use std::io::Read;

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
        pub fn as_component_field(&self) -> &str {
            match self {
                ComponentHint::Source => "sources",
                ComponentHint::Transform => "transforms",
                ComponentHint::Sink => "sinks",
                ComponentHint::Test => "tests",
                ComponentHint::EnrichmentTable => "enrichment_tables",
            }
        }
    }

    /// This trait contains methods that facilitate deserialization of a loader. This includes
    /// loading a type based on a provided `Format`, and processing files.
    pub trait Process {
        fn prepare<R: Read>(&self, input: R) -> Result<(String, Vec<String>), Vec<String>>;

        fn load<R: std::io::Read, T>(
            &self,
            input: R,
            format: Format,
        ) -> Result<(T, Vec<String>), Vec<String>>
        where
            T: serde::de::DeserializeOwned,
        {
            let (value, warnings) = self.prepare(input)?;

            format::deserialize(&value, format).map(|builder| (builder, warnings))
        }

        fn load_dir_into(
            &self,
            path: &Path,
            result: &mut Table,
            recurse: bool,
        ) -> Result<Vec<String>, Vec<String>> {
            let mut errors = Vec::new();
            let mut warnings = Vec::new();
            let readdir = read_dir(path)?;

            let mut files = Vec::new();
            let mut folders = Vec::new();

            for direntry in readdir {
                match direntry {
                    Ok(item) => {
                        let entry = item.path();
                        if entry.is_file() {
                            files.push(entry);
                        } else if entry.is_dir() {
                            folders.push(entry);
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
                let loaded = if recurse {
                    self.load_file_recursive(&entry)
                } else {
                    self.load_file(&entry)
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

        fn load_file(
            &self,
            path: &Path,
        ) -> Result<Option<(String, Table, Vec<String>)>, Vec<String>> {
            if let (Ok(name), Some(file), Ok(format)) = (
                component_name(path),
                open_file(path),
                Format::from_path(path),
            ) {
                self.load(file, format)
                    .map(|(value, warnings)| Some((name, value, warnings)))
            } else {
                Ok(None)
            }
        }

        fn load_file_recursive(
            &self,
            path: &Path,
        ) -> Result<Option<(String, Table, Vec<String>)>, Vec<String>> {
            if let Some((name, mut table, mut warnings)) = self.load_file(path)? {
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

        fn load_dir(
            &self,
            path: &Path,
            recurse: bool,
        ) -> Result<(Table, Vec<String>), Vec<String>> {
            let mut result = Table::new();
            let warnings = self.load_dir_into(path, &mut result, recurse)?;
            Ok((result, warnings))
        }

        fn merge(&mut self, table: Table, hint: Option<ComponentHint>) -> Result<(), Vec<String>>;
    }
}

/// `Loader` represents the public part of the loading interface. Includes methods for loading
/// from files/folders, and accessing the final deserialized `T` value via the `take` method.
pub trait Loader<T>: process::Process
where
    T: serde::de::DeserializeOwned,
{
    /// Consumes Self, and returns the final, deserialized `T`.
    fn take(self) -> T;

    /// Deserializes a file with the provided format, and makes the result available via `take`.
    /// Returns a vector of non-fatal warnings on success, or a vector of error strings on failure.
    fn load_from_file(&mut self, path: &Path) -> Result<Vec<String>, Vec<String>> {
        if let Ok(Some((_, table, warnings))) = self.load_file(path) {
            self.merge(table, None)?;
            Ok(warnings)
        } else {
            Ok(vec![])
        }
    }

    /// Deserializes a dir with the provided format, and makes the result available via `take`.
    /// Returns a vector of non-fatal warnings on success, or a vector of error strings on failure.
    fn load_from_dir(&mut self, path: &Path) -> Result<Vec<String>, Vec<String>> {
        use process::ComponentHint;

        // Paths to process, starting with the current folder, and looking for sub-folders
        // to process namespaced components if applicable. An optional `ComponentHint` is
        // provided, which is passed along to the `load` method to determine how to deserialize.
        let paths = [
            (
                path.join(ComponentHint::Source.as_component_field()),
                ComponentHint::Source,
            ),
            (
                path.join(ComponentHint::Transform.as_component_field()),
                ComponentHint::Transform,
            ),
            (
                path.join(ComponentHint::Sink.as_component_field()),
                ComponentHint::Sink,
            ),
            (
                path.join(ComponentHint::EnrichmentTable.as_component_field()),
                ComponentHint::EnrichmentTable,
            ),
            (
                path.join(ComponentHint::Test.as_component_field()),
                ComponentHint::Test,
            ),
        ];

        // Get files from the root of the folder. These represent top-level config settings,
        // and need to merged down first to represent a more 'complete' config.
        let mut root = Table::new();
        let (table, mut warnings) = self.load_dir(path, false)?;

        // Discard the named part of the path, since these don't form any component names.
        for (_, value) in table {
            if let Value::Table(table) = value {
                println!("inner: {:?}", table);
                merge_into_table(&mut root, table).map_err(|e| vec![e.to_string()])?;
            }
        }

        println!("root: {:?}", root);

        // Merge the 'root' value.
        self.merge(root, None)?;

        for (path, hint) in paths {
            // Sanity check for paths, to ensure we're dealing with a folder. This is necessary
            // because a sub-folder won't generally exist unless the config is namespaced.
            if path.exists() && path.is_dir() {
                let (table, warns) = if matches!(hint, ComponentHint::Transform) {
                    self.load_dir(&path, true)?
                } else {
                    self.load_dir(&path, false)?
                };

                println!("component: {:?}", table);

                self.merge(table, Some(hint))?;

                warnings.extend(warns);
            }
        }

        Ok(warnings)
    }
}

fn merge_values(value: toml::Value, other: toml::Value) -> Result<toml::Value, Vec<String>> {
    serde_toml_merge::merge(value, other).map_err(|err| vec![format!("{}", err)])
}

fn merge_with_value(res: &mut Table, name: String, value: toml::Value) -> Result<(), Vec<String>> {
    if let Some(existing) = res.remove(&name) {
        res.insert(name, merge_values(existing, value)?);
    } else {
        res.insert(name, value);
    }
    Ok(())
}

pub(super) fn deserialize_table<T: serde::de::DeserializeOwned>(
    table: Table,
) -> Result<T, Vec<String>> {
    Value::Table(table)
        .try_into()
        .map_err(|e| vec![e.to_string()])
}
