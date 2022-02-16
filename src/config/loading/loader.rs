use super::{component_name, open_file, read_dir, Format};
use crate::config::format;
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

    #[derive(Debug)]
    pub struct ProcessedFile {
        pub name: String,
        pub input: String,
        pub warnings: Vec<String>,
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
                match self.load_file_recursive(&entry) {
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

            for entry in folders {
                if let Ok(name) = component_name(&entry) {
                    if !result.contains_key(&name) {
                        match self.load_dir(&entry) {
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
                        warnings.extend(self.load_dir_into(&subdir, &mut table)?);
                    }
                }
                Ok(Some((name, table, warnings)))
            } else {
                Ok(None)
            }
        }

        fn load_dir(&self, path: &Path) -> Result<(Table, Vec<String>), Vec<String>> {
            let mut result = Table::new();
            let warnings = self.load_dir_into(path, &mut result)?;
            Ok((result, warnings))
        }

        fn merge(
            &mut self,
            name: String,
            value: Value,
            hint: Option<ComponentHint>,
        ) -> Result<(), Vec<String>>;
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
        if let Ok(Some((name, value, warnings))) = self.load_file(path) {
            self.merge(name, Value::Table(value), None)?;
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
            (path.to_path_buf(), None),
            (
                path.join(ComponentHint::Source.as_component_field()),
                Some(ComponentHint::Source),
            ),
            (
                path.join(ComponentHint::Transform.as_component_field()),
                Some(ComponentHint::Transform),
            ),
            (
                path.join(ComponentHint::Sink.as_component_field()),
                Some(ComponentHint::Sink),
            ),
            (
                path.join(ComponentHint::EnrichmentTable.as_component_field()),
                Some(ComponentHint::EnrichmentTable),
            ),
            (
                path.join(ComponentHint::Test.as_component_field()),
                Some(ComponentHint::Test),
            ),
        ];

        let mut warnings = Vec::new();
        let mut errors = Vec::new();

        for (path, hint) in paths {
            // Sanity check for paths, to ensure we're dealing with a folder. This is necessary
            // because a sub-folder won't generally exist unless the config is namespaced.
            if path.exists() && path.is_dir() {
                match hint {
                    Some(ComponentHint::Transform) => {
                        let (table, warns) = self.load_dir(&path)?;
                        println!("transform: {:?}", table);

                        warnings.extend(warns);
                    }
                    _ => {
                        let readdir = read_dir(&path)?;
                        for res in readdir {
                            match res {
                                Ok(entry) => {
                                    let path = entry.path();
                                    if path.is_file() {
                                        match self.load_file(path.as_path()) {
                                            Ok(None) => continue,
                                            Ok(Some((name, value, warns))) => {
                                                println!("other ({}): {:?}", name, value);
                                                self.merge(name, Value::Table(value), hint)?;
                                                warnings.extend(warns);
                                            }
                                            Err(errs) => {
                                                errors.extend(errs);
                                            }
                                        }
                                    }
                                }
                                Err(err) => {
                                    errors.push(format!(
                                        "Could not read file in config dir: {:?}, {}.",
                                        path, err
                                    ));
                                }
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

pub(super) fn deserialize_value<T: serde::de::DeserializeOwned>(
    value: Value,
) -> Result<T, Vec<String>> {
    value.try_into().map_err(|e| vec![e.to_string()])
}
