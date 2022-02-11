use super::{loader, recursive::merge_with_value};
use crate::config::{
    format, EnrichmentTableOuter, Format, SinkOuter, SourceOuter, TestDefinition, TransformOuter,
};
use indexmap::IndexMap;
use serde_toml_merge::merge_into_table;
use toml::{map::Map, Value};
use vector_core::config::ComponentKey;

pub type SourceMap = Map<String, Value>;

pub struct SourceLoader {
    map: SourceMap,
}

impl SourceLoader {
    pub fn new() -> Self {
        Self { map: Map::new() }
    }
}

impl loader::private::Process<SourceMap> for SourceLoader {
    fn load<R: std::io::Read>(
        &self,
        mut input: R,
        format: Format,
    ) -> Result<(SourceMap, Vec<String>), Vec<String>> {
        let mut source_string = String::new();
        input
            .read_to_string(&mut source_string)
            .map_err(|e| vec![e.to_string()])?;

        format::deserialize(&source_string, format)
    }

    fn add_value(&mut self, value: SourceMap) -> Result<(), Vec<String>> {
        merge_into_table(&mut self.map, value).map_err(|e| vec![e.to_string()])
    }

    fn add_sources(&mut self, sources: IndexMap<ComponentKey, SourceOuter>) {
        merge_with_value(
            &mut self.map,
            "sources".to_owned(),
            toml::Value::try_from(sources).expect("couldn't serialize sources. Please report."),
        )
        .expect("couldn't merge sources. Please report.")
    }

    fn add_transforms(&mut self, transforms: IndexMap<ComponentKey, TransformOuter<String>>) {
        merge_with_value(
            &mut self.map,
            "transforms".to_owned(),
            toml::Value::try_from(transforms)
                .expect("couldn't serialize transforms. Please report."),
        )
        .expect("couldn't merge transforms. Please report.")
    }

    fn add_sink(&mut self, sinks: IndexMap<ComponentKey, SinkOuter<String>>) {
        merge_with_value(
            &mut self.map,
            "sinks".to_owned(),
            toml::Value::try_from(sinks).expect("couldn't serialize sinks. Please report."),
        )
        .expect("couldn't merge sinks. Please report.")
    }

    fn add_enrichment_tables(
        &mut self,
        enrichment_tables: IndexMap<ComponentKey, EnrichmentTableOuter>,
    ) {
        merge_with_value(
            &mut self.map,
            "enrichment_tables".to_owned(),
            toml::Value::try_from(enrichment_tables)
                .expect("couldn't serialize enrichment tables. Please report."),
        )
        .expect("couldn't merge enrichment tables. Please report.")
    }

    fn add_tests(&mut self, tests: IndexMap<ComponentKey, TestDefinition<String>>) {
        let tests = tests.into_iter().map(|(_, test)| test).collect::<Vec<_>>();

        merge_with_value(
            &mut self.map,
            "tests".to_owned(),
            toml::Value::try_from(tests).expect("couldn't serialize tests. Please report."),
        )
        .expect("couldn't merge tests. Please report.")
    }
}

impl loader::Loader<SourceMap> for SourceLoader {
    fn take(self) -> SourceMap {
        self.map
    }
}
