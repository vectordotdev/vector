use super::{loader, prepare_input};
use super::{ComponentHint, Process};
use crate::config::{
    format, ConfigBuilder, EnrichmentTableOuter, Format, SinkOuter, SourceOuter, TestDefinition,
    TransformOuter,
};
use indexmap::IndexMap;
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
    fn load<R: std::io::Read>(
        &mut self,
        name: String,
        input: R,
        format: Format,
        hint: Option<ComponentHint>,
    ) -> Result<Vec<String>, Vec<String>> {
        let (with_vars, warnings) = prepare_input(input)?;

        match hint {
            Some(ComponentHint::Source) => {
                self.builder.sources.insert(
                    ComponentKey::from(name),
                    format::deserialize(&with_vars, format)?,
                );
            }
            Some(ComponentHint::Sink) => {
                self.builder.sinks.insert(
                    ComponentKey::from(name),
                    format::deserialize(&with_vars, format)?,
                );
            }
            Some(ComponentHint::Transform) => {
                self.builder.transforms.insert(
                    ComponentKey::from(name),
                    format::deserialize(&with_vars, format)?,
                );
            }
            Some(ComponentHint::EnrichmentTable) => {
                self.builder.enrichment_tables.insert(
                    ComponentKey::from(name),
                    format::deserialize(&with_vars, format)?,
                );
            }
            Some(ComponentHint::Test) => {
                self.builder
                    .tests
                    .extend(format::deserialize::<Vec<TestDefinition<String>>>(
                        &with_vars, format,
                    )?);
            }
            None => {
                self.builder
                    .append(format::deserialize(&with_vars, format)?)?;
            }
        }

        Ok(warnings)
    }
}

impl loader::Loader<ConfigBuilder> for ConfigBuilderLoader {
    fn take(self) -> ConfigBuilder {
        self.builder
    }
}
