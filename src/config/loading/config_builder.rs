use std::{collections::HashMap, io::Read};

use super::{
    ComponentHint, deserialize_component_map, deserialize_config_map, ignored_fields,
    loader::{ConfigLoader, ConfigScope, LoadMode, ParsedInputs},
    representation::ConfigMap,
};
use crate::config::ConfigBuilder;

#[derive(Debug)]
pub struct ConfigBuilderLoader {
    builder: ConfigBuilder,
    secrets: HashMap<String, String>,
    interpolate_env: bool,
}

impl ConfigBuilderLoader {
    /// Sets whether to interpolate environment variables in the config.
    #[must_use]
    pub const fn interpolate_env(mut self, interpolate: bool) -> Self {
        self.interpolate_env = interpolate;
        self
    }

    /// Sets the secrets map for secret interpolation.
    #[must_use]
    pub fn secrets(mut self, secrets: HashMap<String, String>) -> Self {
        self.secrets = secrets;
        self
    }

    /// Sets whether to allow empty configuration.
    #[must_use]
    pub const fn allow_empty(mut self, allow_empty: bool) -> Self {
        self.builder.allow_empty = allow_empty;
        self
    }

    /// Builds the `ConfigBuilderLoader` and loads configuration from the specified paths.
    ///
    /// # Errors
    ///
    /// Returns error messages if a directory or an opened config file cannot be read, a
    /// referenced environment variable or secret cannot be interpolated, the content cannot be
    /// parsed or deserialized, or values cannot be merged into the config builder. Config files
    /// that cannot be opened are logged and skipped.
    pub fn load_from_paths(
        self,
        config_paths: &[super::ConfigPath],
    ) -> Result<ConfigBuilder, Vec<String>> {
        self.load(ParsedInputs::from_paths(config_paths))
    }

    /// Builds the `ConfigBuilderLoader` and loads configuration from an input reader.
    ///
    /// # Errors
    ///
    /// Returns error messages if `input` cannot be read as UTF-8, a referenced environment
    /// variable or secret cannot be interpolated, the content cannot be parsed or deserialized,
    /// or values cannot be merged into the config builder.
    pub fn load_from_input<R: Read>(
        self,
        input: R,
        format: super::Format,
    ) -> Result<ConfigBuilder, Vec<String>> {
        self.load(ParsedInputs::from_input(input, format))
    }

    fn load(self, mut inputs: ParsedInputs) -> Result<ConfigBuilder, Vec<String>> {
        inputs.interpolate_environment(self.interpolate_env);
        inputs.substitute_secrets(&self.secrets);
        self.load_prepared(&inputs)
    }

    /// Builds from a retained input snapshot whose substitutions are already complete.
    pub(crate) fn load_prepared(
        mut self,
        inputs: &ParsedInputs,
    ) -> Result<ConfigBuilder, Vec<String>> {
        inputs.load_into(&mut self)?;
        Ok(self.builder)
    }
}

impl ConfigLoader for ConfigBuilderLoader {
    const MODE: LoadMode = LoadMode::Configuration;

    fn merge(&mut self, map: ConfigMap, scope: ConfigScope) -> Result<(), Vec<String>> {
        let hint = match scope {
            ConfigScope::Root => None,
            ConfigScope::Component(hint) => Some(hint),
        };
        let ignored = ignored_fields::find(&map, hint);
        match scope {
            ConfigScope::Component(ComponentHint::Source) => {
                self.builder.sources.extend(deserialize_component_map(map)?);
            }
            ConfigScope::Component(ComponentHint::Sink) => {
                self.builder.sinks.extend(deserialize_component_map(map)?);
            }
            ConfigScope::Component(ComponentHint::Transform) => {
                self.builder
                    .transforms
                    .extend(deserialize_component_map(map)?);
            }
            ConfigScope::Component(ComponentHint::EnrichmentTable) => {
                self.builder
                    .enrichment_tables
                    .extend(deserialize_component_map(map)?);
            }
            ConfigScope::Component(hint @ ComponentHint::Test) => {
                // Tests use a root array. Discard filenames while preserving their order.
                let map = ConfigMap::from_iter([(
                    hint.as_component_field().to_owned(),
                    serde_json::Value::Array(map.into_values().collect()),
                )]);
                self.builder
                    .tests
                    .extend(deserialize_config_map::<ConfigBuilder>(map)?.tests);
            }
            ConfigScope::Root => {
                self.builder.append(deserialize_config_map(map)?)?;
            }
        }

        self.builder
            .loading_warnings
            .extend(ignored_fields::warnings(&ignored));
        Ok(())
    }
}

impl Default for ConfigBuilderLoader {
    fn default() -> Self {
        Self {
            builder: ConfigBuilder::default(),
            secrets: HashMap::new(),
            interpolate_env: super::env_var_interpolation_enabled(),
        }
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
