use super::{
    component_name, load_files_from_dir, open_file, read_dir, ComponentKey, Format, Process,
};
use crate::config::{EnrichmentTableOuter, SinkOuter, SourceOuter, TestDefinition, TransformOuter};
use indexmap::IndexMap;
use std::path::Path;

// The loader traits are split into two parts -- an internal `process` mod, that contains
// functionality for deserializing a type `T`, and a `Loader` trait, that provides a public interface
// getting a `T` from a file/directory. The private mod is available to implementors within
// the loading mod, but does not form part of the public interface.
pub(super) mod process {
    use super::*;
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

    pub struct ProcessedFile {
        pub name: String,
        pub input: String,
        pub warnings: Vec<String>,
    }

    /// This trait contains methods that facilitate deserialization of a loader. This includes
    /// loading a type based on a provided `Format`, and processing files.
    pub trait Process {
        fn prepare<R: Read>(&self, input: R) -> Result<(String, Vec<String>), Vec<String>>;

        fn merge(
            &mut self,
            processed_file: &ProcessedFile,
            format: Format,
            hint: Option<ComponentHint>,
        ) -> Result<(), Vec<String>>;

        fn process_file(&mut self, path: &Path) -> Result<Option<ProcessedFile>, Vec<String>> {
            let name = component_name(path)?;
            if let Some(file) = open_file(path) {
                let (input, warnings) = self.prepare(file)?;
                Ok(Some(ProcessedFile {
                    name,
                    input,
                    warnings,
                }))
            } else {
                Ok(None)
            }
        }
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
    fn load_from_file(&mut self, path: &Path, format: Format) -> Result<Vec<String>, Vec<String>> {
        if let Ok(Some(file)) = self.process_file(path) {
            self.merge(&file, format, None);
            Ok(file.warnings)
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
                let readdir = read_dir(&path)?;
                for res in readdir {
                    match res {
                        Ok(dir_entry) => {
                            let entry_path = dir_entry.path();
                            if entry_path.is_file() {
                                // Silently ignore any unknown file formats.
                                if let Ok(format) = Format::from_path(dir_entry.path()) {
                                    match self.process_file(&entry_path) {
                                        Ok(None) => continue,
                                        Ok(Some(file)) => {
                                            self.merge(&file, format, hint);
                                            warnings.extend(file.warnings);
                                        }
                                        Err(errs) => errors.extend(errs),
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

        if errors.is_empty() {
            Ok(warnings)
        } else {
            Err(errors)
        }

        /*
        let sub_folder = path.join("transforms");
        if sub_folder.exists() && sub_folder.is_dir() {
            let (value, warns) = super::recursive::load_dir(&sub_folder)?;
            warnings.extend(warns);
            match toml::Value::Table(value).try_into::<IndexMap<ComponentKey, TransformOuter<_>>>()
            {
                Ok(inner) => self.add_transforms(inner),
                Err(err) => errors.push(format!("Unable to decode transform folder: {:?}", err)),
            }
        }

        if errors.is_empty() {
            Ok(warnings)
        } else {
            Err(errors)
        }

         */
    }
}
