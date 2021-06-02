use super::remap;
use crate::{
    conditions::{Condition, ConditionConfig, ConditionDescription},
    emit,
    event::{Event, VrlTarget},
    internal_events::RemapConditionExecutionError,
};
use datadog_search_syntax::parse;
use serde::{Deserialize, Serialize};
use vrl::{compiler::compile, diagnostic::Formatter, Program, Runtime, Value};
use vrl_parser::ast;

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
        let program = vrl_parser::ast::Program(vec![query_node.into()]);

        // TODO(jean): same to-do as in `remap.rs`

        let functions = vrl_stdlib::all()
            .into_iter()
            .filter(|f| f.identifier() != "del")
            .filter(|f| f.identifier() != "only_fields")
            .collect::<Vec<_>>();

        let compiler = vrl_com

        let program = vrl::compile(&self.source, &functions).map_err(|diagnostics| {
            Formatter::new(&self.source, diagnostics)
                .colored()
                .to_string()
        })?;

        Ok(Box::new(remap::Remap::new(program)))
    }
}

//------------------------------------------------------------------------------

#[cfg(test)]
mod test {
    use std::collections::BTreeMap;

    use super::*;
    use crate::{event::Metric, event::MetricKind, event::MetricValue, log_event};
}
