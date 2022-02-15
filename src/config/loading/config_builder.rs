use super::{loader, prepare_input};
use super::{ComponentHint, Process, ProcessedFile};
use crate::config::{
    format, ConfigBuilder, EnrichmentTableOuter, Format, SinkOuter, SourceOuter, TestDefinition,
    TransformOuter,
};
use indexmap::IndexMap;
use serde::de::DeserializeOwned;
use std::{io::Read, path::Path};
use vector_core::config::ComponentKey;

pub struct ConfigBuilderLoader {
    builder: ConfigBuilder,
}

impl ConfigBuilderLoader {
    pub fn new() -> Self {
        Self {
            builder: ConfigBuilder::default(),
        }
    }
}

impl Process for ConfigBuilderLoader {
    fn prepare<R: Read>(&self, input: R) -> Result<(String, Vec<String>), Vec<String>> {
        prepare_input(input)
    }

    fn merge(
        &mut self,
        file: &ProcessedFile,
        format: Format,
        hint: Option<ComponentHint>,
    ) -> Result<(), Vec<String>> {
        let component_key = ComponentKey::from(file.name.as_str());

        match hint {
            Some(ComponentHint::Source) => {
                self.builder
                    .sources
                    .insert(component_key, format::deserialize(&file.input, format)?);
            }
            Some(ComponentHint::Sink) => {
                self.builder
                    .sinks
                    .insert(component_key, format::deserialize(&file.input, format)?);
            }
            Some(ComponentHint::Transform) => {
                self.builder
                    .transforms
                    .insert(component_key, format::deserialize(&file.input, format)?);
            }
            Some(ComponentHint::EnrichmentTable) => {
                self.builder
                    .enrichment_tables
                    .insert(component_key, format::deserialize(&file.input, format)?);
            }
            Some(ComponentHint::Test) => {
                self.builder
                    .tests
                    .extend(format::deserialize::<Vec<TestDefinition<String>>>(
                        &file.input,
                        format,
                    )?);
            }
            None => {
                self.builder
                    .append(format::deserialize(&file.input, format)?)?;
            }
        };

        Ok(())
    }
}

impl loader::Loader<ConfigBuilder> for ConfigBuilderLoader {
    fn take(self) -> ConfigBuilder {
        self.builder
    }
}
