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
    secrets: Option<HashMap<String, String>>,
    interpolate_env: bool,
}

impl ConfigBuilderLoader {
    /// Creates a new builder with default settings.
    /// By default, environment variable interpolation is enabled.
    pub fn new() -> Self {
        Self {
            builder: ConfigBuilder::default(),
            secrets: None,
            interpolate_env: true,
        }
    }

    /// Sets whether to interpolate environment variables in the config.
    pub const fn interpolate_env(mut self, interpolate: bool) -> Self {
        self.interpolate_env = interpolate;
        self
    }

    /// Sets the secrets map for secret interpolation.
    pub fn secrets(mut self, secrets: HashMap<String, String>) -> Self {
        self.secrets = Some(secrets);
        self
    }

    /// Builds the ConfigBuilderLoader and loads configuration from the specified paths.
    pub fn load_from_paths(mut self, config_paths: &[super::ConfigPath]) -> Result<ConfigBuilder, Vec<String>> {
        use super::{ConfigPath, Format, Loader};

        let mut errors = Vec::new();

        for config_path in config_paths {
            match config_path {
                ConfigPath::File(path, format_hint) => {
                    match self.load_from_file(
                        path,
                        format_hint
                            .or_else(move || Format::from_path(path).ok())
                            .unwrap_or_default(),
                    ) {
                        Ok(()) => {}
                        Err(errs) => errors.extend(errs),
                    };
                }
                ConfigPath::Dir(path) => {
                    match self.load_from_dir(path) {
                        Ok(()) => {}
                        Err(errs) => errors.extend(errs),
                    };
                }
            }
        }

        if errors.is_empty() {
            Ok(self.take())
        } else {
            Err(errors)
        }
    }

    /// Builds the ConfigBuilderLoader and loads configuration from an input reader.
    pub fn load_from_input<R: Read>(
        mut self,
        input: R,
        format: super::Format,
    ) -> Result<ConfigBuilder, Vec<String>> {
        use super::Loader;

        self.load_from_str(input, format).map(|_| self.take())
    }
}

impl Default for ConfigBuilderLoader {
    fn default() -> Self {
        Self::new()
    }
}

impl Process for ConfigBuilderLoader {
    /// Prepares input for a `ConfigBuilder` by interpolating environment variables.
    fn prepare<R: Read>(&mut self, input: R) -> Result<String, Vec<String>> {
        let prepared_input = prepare_input(input, self.interpolate_env)?;
        let prepared_input = self
            .secrets
            .as_ref()
            .map(|s| secret::interpolate(&prepared_input, s))
            .unwrap_or(Ok(prepared_input))?;
        Ok(prepared_input)
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
