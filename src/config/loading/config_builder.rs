use std::{collections::HashMap, io::Read};

use indexmap::IndexMap;
use toml::value::Table;

use super::{ComponentHint, Process, deserialize_table, loader, prepare_input, secret};
use crate::config::{
    ComponentKey, ConfigBuilder, EnrichmentTableOuter, SinkOuter, SourceOuter, TestDefinition,
    TransformOuter,
};

#[derive(Debug)]
pub struct ConfigBuilderLoader {
    builder: ConfigBuilder,
    secrets: HashMap<String, String>,
    interpolate_env: bool,
}

impl ConfigBuilderLoader {
    /// Sets whether to interpolate environment variables in the config.
    pub const fn interpolate_env(mut self, interpolate: bool) -> Self {
        self.interpolate_env = interpolate;
        self
    }

    /// Sets the secrets map for secret interpolation.
    pub fn secrets(mut self, secrets: HashMap<String, String>) -> Self {
        self.secrets = secrets;
        self
    }

    /// Sets whether to allow empty configuration.
    pub const fn allow_empty(mut self, allow_empty: bool) -> Self {
        self.builder.allow_empty = allow_empty;
        self
    }

    /// Builds the ConfigBuilderLoader and loads configuration from the specified paths.
    pub fn load_from_paths(
        self,
        config_paths: &[super::ConfigPath],
    ) -> Result<ConfigBuilder, Vec<String>> {
        super::loader_from_paths(self, config_paths)
    }

    /// Builds the ConfigBuilderLoader and loads configuration from an input reader.
    pub fn load_from_input<R: Read>(
        self,
        input: R,
        format: super::Format,
    ) -> Result<ConfigBuilder, Vec<String>> {
        super::loader_from_input(self, input, format)
    }
}

impl Default for ConfigBuilderLoader {
    /// Creates a new builder with default settings.
    /// By default, environment variable interpolation is enabled.
    fn default() -> Self {
        Self {
            builder: ConfigBuilder::default(),
            secrets: HashMap::new(),
            interpolate_env: true,
        }
    }
}

impl Process for ConfigBuilderLoader {
    /// Prepares input for a `ConfigBuilder` by interpolating environment variables.
    fn prepare<R: Read>(&mut self, input: R) -> Result<String, Vec<String>> {
        let prepared_input = prepare_input(input, self.interpolate_env)?;
        Ok(if self.secrets.is_empty() {
            prepared_input
        } else {
            secret::interpolate(&prepared_input, &self.secrets)?
        })
    }

    /// Merge a TOML `Table` with a `ConfigBuilder`. Component types extend specific keys.
    fn merge(&mut self, table: Table, hint: Option<ComponentHint>) -> Result<(), Vec<String>> {
        match hint {
            Some(ComponentHint::Source) => {
                self.builder.sources.extend(deserialize_table::<
                    IndexMap<ComponentKey, SourceOuter>,
                >(table)?);
            }
            Some(ComponentHint::Sink) => {
                self.builder.sinks.extend(
                    deserialize_table::<IndexMap<ComponentKey, SinkOuter<_>>>(table)?,
                );
            }
            Some(ComponentHint::Transform) => {
                self.builder.transforms.extend(deserialize_table::<
                    IndexMap<ComponentKey, TransformOuter<_>>,
                >(table)?);
            }
            Some(ComponentHint::EnrichmentTable) => {
                self.builder.enrichment_tables.extend(deserialize_table::<
                    IndexMap<ComponentKey, EnrichmentTableOuter<_>>,
                >(table)?);
            }
            Some(ComponentHint::Test) => {
                // This serializes to a `Vec<TestDefinition<_>>`, so we need to first expand
                // it to an ordered map, and then pull out the value, ignoring the keys.
                self.builder.tests.extend(
                    deserialize_table::<IndexMap<String, TestDefinition<String>>>(table)?
                        .into_iter()
                        .map(|(_, test)| test),
                );
            }
            None => {
                self.builder.append(deserialize_table(table)?)?;
            }
        };

        Ok(())
    }
}

impl loader::Loader<ConfigBuilder> for ConfigBuilderLoader {
    /// Returns the resulting `ConfigBuilder`.
    fn take(self) -> ConfigBuilder {
        self.builder
    }
}

#[cfg(all(
    test,
    feature = "sinks-elasticsearch",
    feature = "transforms-sample",
    feature = "sources-demo_logs",
    feature = "sinks-console"
))]
mod tests {
    use std::path::PathBuf;

    use super::ConfigBuilderLoader;
    use crate::config::{ComponentKey, ConfigPath};

    #[test]
    fn load_namespacing_folder() {
        let path = PathBuf::from(".")
            .join("tests")
            .join("namespacing")
            .join("success");
        let configs = vec![ConfigPath::Dir(path)];
        let builder = ConfigBuilderLoader::default()
            .interpolate_env(true)
            .load_from_paths(&configs)
            .unwrap();
        assert!(
            builder
                .transforms
                .contains_key(&ComponentKey::from("apache_parser"))
        );
        assert!(
            builder
                .sources
                .contains_key(&ComponentKey::from("apache_logs"))
        );
        assert!(
            builder
                .sinks
                .contains_key(&ComponentKey::from("es_cluster"))
        );
        assert_eq!(builder.tests.len(), 2);
    }

    #[test]
    fn load_namespacing_ignore_invalid() {
        let path = PathBuf::from(".")
            .join("tests")
            .join("namespacing")
            .join("ignore-invalid");
        let configs = vec![ConfigPath::Dir(path)];
        ConfigBuilderLoader::default()
            .interpolate_env(true)
            .load_from_paths(&configs)
            .unwrap();
    }

    #[test]
    fn load_directory_ignores_unknown_file_formats() {
        let path = PathBuf::from(".")
            .join("tests")
            .join("config-dir")
            .join("ignore-unknown");
        let configs = vec![ConfigPath::Dir(path)];
        ConfigBuilderLoader::default()
            .interpolate_env(true)
            .load_from_paths(&configs)
            .unwrap();
    }

    #[test]
    fn load_directory_globals() {
        let path = PathBuf::from(".")
            .join("tests")
            .join("config-dir")
            .join("globals");
        let configs = vec![ConfigPath::Dir(path)];
        ConfigBuilderLoader::default()
            .interpolate_env(true)
            .load_from_paths(&configs)
            .unwrap();
    }

    #[test]
    fn load_directory_globals_duplicates() {
        let path = PathBuf::from(".")
            .join("tests")
            .join("config-dir")
            .join("globals-duplicate");
        let configs = vec![ConfigPath::Dir(path)];
        ConfigBuilderLoader::default()
            .interpolate_env(true)
            .load_from_paths(&configs)
            .unwrap();
    }
}
