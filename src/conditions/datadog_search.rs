use super::remap;
use crate::conditions::{Condition, ConditionConfig, ConditionDescription};
use datadog_search_syntax::{compile, parse, Builder};
use serde::{Deserialize, Serialize};
use vrl::diagnostic::Formatter;

#[derive(Deserialize, Serialize, Debug, Default, Clone, PartialEq)]
pub struct DatadogSearchConfig {
    pub source: String,
}

inventory::submit! {
    ConditionDescription::new::<DatadogSearchConfig>("datadog_search")
}

impl_generate_config_from_default!(DatadogSearchConfig);

#[typetag::serde(name = "datadog_search")]
impl ConditionConfig for DatadogSearchConfig {
    fn build(&self) -> crate::Result<Box<dyn Condition>> {
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
    use serde_json::json;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<DatadogSearchConfig>();
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
            // Tag exists (negate).
            (
                "NOT _exists_:a",
                log_event!["tags" => vec!["b:foo"]],
                log_event!("tags" => vec!["a:foo"]),
            ),
            // Tag exists (negate w/-).
            (
                "-_exists_:a",
                log_event!["tags" => vec!["b:foo"]],
                log_event!["tags" => vec!["a:foo"]],
            ),
            // Facet exists.
            (
                "_exists_:@b",
                log_event!["custom" => json!({"b": "foo"})],
                log_event!["custom" => json!({"a": "foo"})],
            ),
            // Facet exists (negate).
            (
                "NOT _exists_:@b",
                log_event!["custom" => json!({"a": "foo"})],
                log_event!["custom" => json!({"b": "foo"})],
            ),
            // Facet exists (negate w/-).
            (
                "-_exists_:@b",
                log_event!["custom" => json!({"a": "foo"})],
                log_event!["custom" => json!({"b": "foo"})],
            ),
            // Tag doesn't exist.
            (
                "_missing_:a",
                log_event![],
                log_event!["tags" => vec!["a:foo"]],
            ),
            // Tag doesn't exist (negate).
            (
                "NOT _missing_:a",
                log_event!["tags" => vec!["a:foo"]],
                log_event![],
            ),
            // Tag doesn't exist (negate w/-).
            (
                "-_missing_:a",
                log_event!["tags" => vec!["a:foo"]],
                log_event![],
            ),
            // Facet doesn't exist.
            (
                "_missing_:@b",
                log_event!["custom" => json!({"a": "foo"})],
                log_event!["custom" => json!({"b": "foo"})],
            ),
            // Facet doesn't exist (negate).
            (
                "NOT _missing_:@b",
                log_event!["custom" => json!({"b": "foo"})],
                log_event!["custom" => json!({"a": "foo"})],
            ),
            // Facet doesn't exist (negate w/-).
            (
                "-_missing_:@b",
                log_event!["custom" => json!({"b": "foo"})],
                log_event!["custom" => json!({"a": "foo"})],
            ),
            // Keyword.
            ("bla", log_event!["message" => "bla"], log_event![]),
            (
                "foo",
                log_event!["message" => r#"{"key": "foo"}"#],
                log_event![],
            ),
            (
                "bar",
                log_event!["message" => r#"{"nested": {"value": ["foo", "bar"]}}"#],
                log_event![],
            ),
        ];

        for (source, pass, fail) in checks {
            let config = DatadogSearchConfig {
                source: source.to_owned(),
            };

            // Every query should build successfully.
            let cond = config
                .build()
                .unwrap_or_else(|_| panic!("build failed: {}", source));

            assert!(
                cond.check_with_context(&pass).is_ok(),
                "should pass: {}\nevent: {:?}",
                source,
                pass
            );
            assert!(
                cond.check_with_context(&fail).is_err(),
                "should fail: {}\nevent: {:?}",
                source,
                fail
            );
        }
    }
}
