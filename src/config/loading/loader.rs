use super::{component_name, load_files_from_dir, open_file};
use crate::config::{Format, TransformOuter};
use indexmap::IndexMap;
use std::path::Path;
use vector_core::config::ComponentKey;

// The loader traits are split into two parts -- a 'private' mod, that contains functionality
// for deserializing a type `T`, and a `Loader` trait, that provides a public interface
// getting a `T` from a file/directory. The private mod is available to implementors within
// the loading mod, but does not form part of the public interface.
pub(super) mod private {
    use super::{component_name, open_file};
    use crate::config::{
        EnrichmentTableOuter, Format, SinkOuter, SourceOuter, TestDefinition, TransformOuter,
    };
    use indexmap::IndexMap;
    use std::path::Path;
    use vector_core::config::ComponentKey;

    /// This trait contains methods that facilitate deserialization of type `T`. This includes
    /// loading a type based on a provided `Format`, adding root values and adding IndexMaps of
    /// components into named parts of the graph.
    pub trait Process<T>
    where
        T: serde::de::DeserializeOwned,
    {
        /// Takes an input `R` and the format, and returns a Result containing the deserialized
        /// payload and a vector of deserialization warnings, or an Error containing a vector of
        /// error strings.
        fn load<R: std::io::Read>(
            &self,
            input: R,
            format: Format,
        ) -> Result<(T, Vec<String>), Vec<String>>;

        /// Receives the deserialized value `T`, which can be stored by the implementor, and retrieved
        /// using the `Loader::take` method.
        fn add_value(&mut self, value: T) -> Result<(), Vec<String>>;

        /// Add an IndexMap of sources. A typical use-case would be inserting into a known part
        /// of the graph representing source components.
        fn add_sources(&mut self, sources: IndexMap<ComponentKey, SourceOuter>);

        /// Add an IndexMap of transforms.
        fn add_transforms(&mut self, transforms: IndexMap<ComponentKey, TransformOuter<String>>);

        /// Add an IndexMap of sinks.
        fn add_sink(&mut self, sinks: IndexMap<ComponentKey, SinkOuter<String>>);

        /// Add an IndexMap of enrichment tables.
        fn add_enrichment_tables(
            &mut self,
            enrichment_tables: IndexMap<ComponentKey, EnrichmentTableOuter>,
        );

        /// Add an IndexMap of tests.
        fn add_tests(&mut self, component: IndexMap<ComponentKey, TestDefinition<String>>);

        /// Process an individual file. Assumes that the &Path provided is a file. This method
        /// both opens the file and calls out to `load` to serialize it as `T`, before sending
        /// it to a post-processor to decide how to store it.
        fn process_file(
            &mut self,
            path: &Path,
            format: Format,
        ) -> Result<Option<(String, T, Vec<String>)>, Vec<String>> {
            let name = component_name(path)?;
            if let Some(file) = open_file(path) {
                let (component, warnings): (T, Vec<String>) = self.load(file, format)?;
                Ok(Some((name, component, warnings)))
            } else {
                Ok(None)
            }
        }
    }
}

/// `Loader` represents the public part of the loading interface. Includes methods for loading
/// from files/folders, and accessing the final deserialized value via the `take` method.
pub trait Loader<T>: private::Process<T>
where
    T: serde::de::DeserializeOwned,
{
    /// Consumes Self, and returns the final, deserialized `T`.
    fn take(self) -> T;

    /// Deserializes a file with the provided format, and makes the result available via `take`.
    /// Returns a vector of non-fatal warnings on success, or a vector of error strings on failure.
    fn load_from_file(&mut self, path: &Path, format: Format) -> Result<Vec<String>, Vec<String>> {
        if let Some((_, value, warnings)) = self.process_file(path, format)? {
            self.add_value(value)?;
            Ok(warnings)
        } else {
            Ok(vec![])
        }
    }

    /// Deserializes a dir with the provided format, and makes the result available via `take`.
    /// Returns a vector of non-fatal warnings on success, or a vector of error strings on failure.
    fn load_from_dir(&mut self, path: &Path) -> Result<Vec<String>, Vec<String>> {
        let mut errors = Vec::new();

        let (root, mut warnings) = load_files_from_dir(path)?;

        // Pull out each serialized file, and pass each to the value processor. This will mean
        // different things to different implementors, but will generally involve merging
        // values against a 'base'.
        for (_, value) in root {
            self.add_value(value)?;
        }

        // If a sub-folder matches an opinionated "namespaced" name, dive into the folder
        // (one level) and pass the deserialized values into named methods so that we can
        // treat them as known components.
        let sub_folder = path.join("enrichment_tables");
        if sub_folder.exists() && sub_folder.is_dir() {
            match load_files_from_dir(&sub_folder) {
                Ok((inner, warns)) => {
                    warnings.extend(warns);
                    self.add_enrichment_tables(inner);
                }
                Err(errs) => errors.extend(errs),
            }
        }

        let sub_folder = path.join("sinks");
        if sub_folder.exists() && sub_folder.is_dir() {
            match load_files_from_dir(&sub_folder) {
                Ok((inner, warns)) => {
                    warnings.extend(warns);
                    self.add_sink(inner);
                }
                Err(errs) => errors.extend(errs),
            }
        }

        let sub_folder = path.join("sources");
        if sub_folder.exists() && sub_folder.is_dir() {
            match load_files_from_dir(&sub_folder) {
                Ok((inner, warns)) => {
                    warnings.extend(warns);
                    self.add_sources(inner);
                }
                Err(errs) => errors.extend(errs),
            }
        }

        let sub_folder = path.join("tests");
        if sub_folder.exists() && sub_folder.is_dir() {
            match load_files_from_dir(&sub_folder) {
                Ok((inner, warns)) => {
                    warnings.extend(warns);
                    self.add_tests(inner);
                }
                Err(errs) => errors.extend(errs),
            }
        }

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
    }
}
