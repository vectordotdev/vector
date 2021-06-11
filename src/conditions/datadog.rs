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

#[cfg(test)]
mod test {
    use super::*;
    use crate::log_event;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<DatadogConfig>();
    }

    #[test]
    fn check_datadog() {
        let checks = vec![
            // Tag exists.
            (
                "_exists_:a",                        // Source
                log_event!["tags" => vec!["a:foo"]], // Pass
                log_event!["tags" => vec!["b:foo"]], // Fail
            ),
        ];

        for (source, pass, fail) in checks {
            let config = DatadogConfig {
                source: source.to_owned(),
            };

            // Every query should build successfully.
            let cond = config
                .build()
                .unwrap_or_else(|_| panic!("build failed: {}", source));

            assert!(cond.check_with_context(&pass).is_ok());
            assert!(cond.check_with_context(&fail).is_err());
        }
    }
}
