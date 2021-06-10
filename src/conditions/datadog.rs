use super::remap;
use crate::conditions::{Condition, ConditionConfig, ConditionDescription};
use datadog_search_syntax::{compile, parse, Builder};
use serde::{Deserialize, Serialize};
use vrl::diagnostic::Formatter;

#[derive(Deserialize, Serialize, Debug, Default, Clone, PartialEq)]
pub struct DatadogConfig {
    pub source: String,
}

inventory::submit! {
    ConditionDescription::new::<DatadogConfig>("datadog")
}

impl_generate_config_from_default!(DatadogConfig);

#[typetag::serde(name = "datadog")]
impl ConditionConfig for DatadogConfig {
    fn build(&self) -> crate::Result<Box<dyn Condition>> {
        // Attempt to parse the Datadog search query.
        let query_node = parse(&self.source)?;

        let builder = Builder::new();

        let program = compile(builder.build(&query_node)).map_err(|diagnostics| {
            Formatter::new(&self.source, diagnostics)
                .colored()
                .to_string()
        })?;

        Ok(Box::new(remap::Remap::new(program)))
    }
}

//------------------------------------------------------------------------------
