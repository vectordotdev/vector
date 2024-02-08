#![allow(missing_docs)]
use vector_lib::configurable::configurable_component;

use crate::event::Event;

mod datadog_search;
pub(crate) mod is_log;
pub(crate) mod is_metric;
pub(crate) mod is_trace;
mod vrl;

pub use self::datadog_search::{DatadogSearchConfig, DatadogSearchRunner};
pub use self::vrl::VrlConfig;
use self::{
    is_log::{check_is_log, check_is_log_with_context},
    is_metric::{check_is_metric, check_is_metric_with_context},
    is_trace::{check_is_trace, check_is_trace_with_context},
    vrl::Vrl,
};

#[derive(Debug, Clone)]
#[allow(clippy::large_enum_variant)]
pub enum Condition {
    /// Matches an event if it is a log.
    IsLog,

    /// Matches an event if it is a metric.
    IsMetric,

    /// Matches an event if it is a trace.
    IsTrace,

    /// Matches an event with a [Vector Remap Language](https://vector.dev/docs/reference/vrl) (VRL) [boolean expression](https://vector.dev/docs/reference/vrl#boolean-expressions).
    Vrl(Vrl),

    /// Matches an event with a [Datadog Search](https://docs.datadoghq.com/logs/explorer/search_syntax/) query.
    DatadogSearch(DatadogSearchRunner),

    /// Matches any event.
    ///
    /// Used only for internal testing.
    AlwaysPass,

    /// Matches no event.
    ///
    /// Used only for internal testing.
    AlwaysFail,
}

impl Condition {
    /// Checks if a condition is true.
    ///
    /// The event should not be modified, it is only mutable so it can be passed into VRL, but VRL type checking prevents mutation.
    #[allow(dead_code)]
    pub fn check(&self, e: Event) -> (bool, Event) {
        match self {
            Condition::IsLog => check_is_log(e),
            Condition::IsMetric => check_is_metric(e),
            Condition::IsTrace => check_is_trace(e),
            Condition::Vrl(x) => x.check(e),
            Condition::DatadogSearch(x) => x.check(e),
            Condition::AlwaysPass => (true, e),
            Condition::AlwaysFail => (false, e),
        }
    }

    /// Checks if a condition is true, with a `Result`-oriented return for easier composition.
    ///
    /// This can be mildly expensive for conditions that do not often match, as it allocates a string for the error
    /// case. As such, it should typically be avoided in hot paths.
    pub(crate) fn check_with_context(&self, e: Event) -> (Result<(), String>, Event) {
        match self {
            Condition::IsLog => check_is_log_with_context(e),
            Condition::IsMetric => check_is_metric_with_context(e),
            Condition::IsTrace => check_is_trace_with_context(e),
            Condition::Vrl(x) => x.check_with_context(e),
            Condition::DatadogSearch(x) => x.check_with_context(e),
            Condition::AlwaysPass => (Ok(()), e),
            Condition::AlwaysFail => (Ok(()), e),
        }
    }
}

/// An event matching condition.
///
/// Many methods exist for matching events, such as using a VRL expression, a Datadog Search query string,
/// or hard-coded matchers like "must be a metric" or "fields A, B, and C must match these constraints".
///
/// They can specified with an enum-style notation:
///
/// ```toml
/// condition.type = 'datadog_search'
/// condition.source = 'NOT "foo"'
/// ```
#[configurable_component]
#[derive(Clone, Debug)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ConditionConfig {
    /// Matches an event if it is a log.
    #[configurable(metadata(docs::hidden))]
    IsLog,

    /// Matches an event if it is a metric.
    #[configurable(metadata(docs::hidden))]
    IsMetric,

    /// Matches an event if it is a trace.
    #[configurable(metadata(docs::hidden))]
    IsTrace,

    /// Matches an event with a [Vector Remap Language](https://vector.dev/docs/reference/vrl) (VRL) [boolean expression](https://vector.dev/docs/reference/vrl#boolean-expressions).
    Vrl(VrlConfig),

    /// Matches an event with a [Datadog Search](https://docs.datadoghq.com/logs/explorer/search_syntax/) query.
    DatadogSearch(DatadogSearchConfig),
}

impl ConditionConfig {
    pub fn build(
        &self,
        enrichment_tables: &vector_lib::enrichment::TableRegistry,
    ) -> crate::Result<Condition> {
        match self {
            ConditionConfig::IsLog => Ok(Condition::IsLog),
            ConditionConfig::IsMetric => Ok(Condition::IsMetric),
            ConditionConfig::IsTrace => Ok(Condition::IsTrace),
            ConditionConfig::Vrl(x) => x.build(enrichment_tables),
            ConditionConfig::DatadogSearch(x) => x.build(enrichment_tables),
        }
    }
}

pub trait Conditional: std::fmt::Debug {
    /// Checks if a condition is true.
    ///
    /// The event should not be modified, it is only mutable so it can be passed into VRL, but VRL type checking prevents mutation.
    fn check(&self, event: Event) -> (bool, Event);

    /// Checks if a condition is true, with a `Result`-oriented return for easier composition.
    ///
    /// This can be mildly expensive for conditions that do not often match, as it allocates a string for the error
    /// case. As such, it should typically be avoided in hot paths.
    fn check_with_context(&self, e: Event) -> (Result<(), String>, Event) {
        let (result, event) = self.check(e);
        if result {
            (Ok(()), event)
        } else {
            (Err("condition failed".into()), event)
        }
    }
}

pub trait ConditionalConfig: std::fmt::Debug + Send + Sync + dyn_clone::DynClone {
    fn build(
        &self,
        enrichment_tables: &vector_lib::enrichment::TableRegistry,
    ) -> crate::Result<Condition>;
}

dyn_clone::clone_trait_object!(ConditionalConfig);

/// An event matching condition.
///
/// Many methods exist for matching events, such as using a VRL expression, a Datadog Search query string,
/// or hard-coded matchers like "must be a metric" or "fields A, B, and C must match these constraints".
///
/// As VRL is the most common way to apply conditions to events, this type provides a shortcut to define VRL expressions
/// directly in the configuration by passing the VRL expression as a string:
///
/// ```toml
/// condition = '.message == "hooray"'
/// ```
///
/// When other condition types are required, they can be specified with an enum-style notation:
///
/// ```toml
/// condition.type = 'datadog_search'
/// condition.source = 'NOT "foo"'
/// ```
#[configurable_component]
#[derive(Clone, Debug)]
#[configurable(metadata(docs::type_override = "condition"))]
#[serde(untagged)]
pub enum AnyCondition {
    /// A [Vector Remap Language](https://vector.dev/docs/reference/vrl) (VRL) [boolean expression](https://vector.dev/docs/reference/vrl#boolean-expressions).
    String(String),

    /// A fully-specified condition.
    Map(ConditionConfig),
}

impl AnyCondition {
    pub fn build(
        &self,
        enrichment_tables: &vector_lib::enrichment::TableRegistry,
    ) -> crate::Result<Condition> {
        match self {
            AnyCondition::String(s) => {
                let vrl_config = VrlConfig {
                    source: s.clone(),
                    runtime: Default::default(),
                };
                vrl_config.build(enrichment_tables)
            }
            AnyCondition::Map(m) => m.build(enrichment_tables),
        }
    }
}

impl From<ConditionConfig> for AnyCondition {
    fn from(config: ConditionConfig) -> Self {
        Self::Map(config)
    }
}

#[cfg(test)]
mod tests {
    use indoc::indoc;
    use serde::Deserialize;

    use super::*;

    #[derive(Deserialize, Debug)]
    struct Test {
        condition: AnyCondition,
    }

    #[test]
    fn deserialize_anycondition_default() {
        let conf: Test = toml::from_str(r#"condition = ".nork == false""#).unwrap();
        assert_eq!(
            r#"String(".nork == false")"#,
            format!("{:?}", conf.condition)
        )
    }

    #[test]
    fn deserialize_anycondition_vrl() {
        let conf: Test = toml::from_str(indoc! {r#"
            condition.type = "vrl"
            condition.source = '.nork == true'
        "#})
        .unwrap();

        assert_eq!(
            r#"Map(Vrl(VrlConfig { source: ".nork == true", runtime: Ast }))"#,
            format!("{:?}", conf.condition)
        )
    }
}
