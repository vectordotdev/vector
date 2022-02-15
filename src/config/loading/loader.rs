use super::{
    component_name, load_files_from_dir, open_file, read_dir, ComponentHint, Format, Process,
};
use crate::config::TransformOuter;
use indexmap::IndexMap;
use std::path::{Path, PathBuf};
use vector_core::config::ComponentKey;

/// `Loader` represents the public part of the loading interface. Includes methods for loading
/// from files/folders, and accessing the final deserialized value via the `take` method.
pub trait Loader<T>: Process
where
    T: serde::de::DeserializeOwned,
{
    /// Consumes Self, and returns the final, deserialized `T`.
    fn take(self) -> T;

    /// Deserializes a file with the provided format, and makes the result available via `take`.
    /// Returns a vector of non-fatal warnings on success, or a vector of error strings on failure.
    fn load_from_file(&mut self, path: &Path, format: Format) -> Result<Vec<String>, Vec<String>> {
        if let Ok(Some(warnings)) = self.process_file::<T>(path, format, None) {
            Ok(warnings)
        } else {
            Ok(vec![])
        }
    }

    /// Deserializes a dir with the provided format, and makes the result available via `take`.
    /// Returns a vector of non-fatal warnings on success, or a vector of error strings on failure.
    fn load_from_dir(&mut self, path: &Path) -> Result<Vec<String>, Vec<String>> {
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
            if path.exists() && path.is_dir() {
                let readdir = read_dir(&path)?;
                for res in readdir {
                    match res {
                        Ok(dir_entry) => {
                            let entry_path = dir_entry.path();
                            if entry_path.is_file() {
                                // skip any unknown file formats
                                if let Ok(format) = Format::from_path(dir_entry.path()) {
                                    match self.process_file::<T>(&entry_path, format, hint) {
                                        Ok(None) => continue,
                                        Ok(Some(warns)) => {
                                            warnings.extend(warns);
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
