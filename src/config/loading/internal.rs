use super::{component_name, open_file};
use crate::config::{
    EnrichmentTableOuter, Format, SinkOuter, SourceOuter, TestDefinition, TransformOuter,
};
use indexmap::IndexMap;
use std::path::Path;
use vector_core::config::ComponentKey;

// The loader traits are split into two parts -- an internal`process` mod, that contains
// functionality for deserializing a type `T`, and a `Loader` trait, that provides a public interface
// getting a `T` from a file/directory. The private mod is available to implementors within
// the loading mod, but does not form part of the public interface.
pub(super) mod process {
    use super::*;

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
            match hint {
                ComponentHint::Source => "sources",
                ComponentHint::Transform => "transforms",
                ComponentHint::Sink => "sinks",
                ComponentHint::Test => "tests",
                ComponentHint::EnrichmentTable => "enrichment_tables",
            }
        }
    }

    /// This trait contains methods that facilitate deserialization of type `T`. This includes
    /// loading a type based on a provided `Format`, adding root values and adding IndexMaps of
    /// components into named parts of the graph.
    pub trait Process {
        /// Takes an input `R` and the format, and returns a Result containing the deserialized
        /// payload and a vector of deserialization warnings, or an Error containing a vector of
        /// error strings.
        fn load<R: std::io::Read>(
            &mut self,
            name: String,
            input: R,
            format: Format,
            hint: Option<ComponentHint>,
        ) -> Result<Vec<String>, Vec<String>>;

        /// Process an individual file. Assumes that the &Path provided is a file. This method
        /// both opens the file and calls out to `load` to serialize it as `T`, before sending
        /// it to a post-processor to decide how to store it.
        fn process_file<T: serde::de::DeserializeOwned>(
            &mut self,
            path: &Path,
            format: Format,
            hint: Option<ComponentHint>,
        ) -> Result<Option<Vec<String>>, Vec<String>> {
            let name = component_name(path)?;
            if let Some(file) = open_file(path) {
                let warnings = self.load(name, file, format, hint)?;
                Ok(Some(warnings))
            } else {
                Ok(None)
            }
        }
    }
}
