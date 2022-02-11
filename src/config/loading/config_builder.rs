use super::{loader, prepare_input};
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

impl loader::private::Process<ConfigBuilder> for ConfigBuilderLoader {
    fn load<R: std::io::Read>(
        &self,
        input: R,
        format: Format,
    ) -> Result<(ConfigBuilder, Vec<String>), Vec<String>> {
        let (with_vars, warnings) = prepare_input(input)?;

        format::deserialize(&with_vars, format).map(|builder| (builder, warnings))
    }

    fn add_value(&mut self, value: ConfigBuilder) -> Result<(), Vec<String>> {
        self.builder.append(value)
    }

    fn add_sources(&mut self, sources: IndexMap<ComponentKey, SourceOuter>) {
        self.builder.sources.extend(sources)
    }

    fn add_transforms(&mut self, component: IndexMap<ComponentKey, TransformOuter<String>>) {
        self.builder.transforms.extend(component);
    }

    fn add_sink(&mut self, component: IndexMap<ComponentKey, SinkOuter<String>>) {
        self.builder.sinks.extend(component);
    }

    fn add_enrichment_tables(&mut self, component: IndexMap<ComponentKey, EnrichmentTableOuter>) {
        self.builder.enrichment_tables.extend(component);
    }

    fn add_tests(&mut self, component: IndexMap<ComponentKey, TestDefinition<String>>) {
        self.builder
            .tests
            .extend(component.into_iter().map(|(_, value)| value));
    }
}

impl loader::Loader<ConfigBuilder> for ConfigBuilderLoader {
    fn take(self) -> ConfigBuilder {
        self.builder
    }
}
