use std::sync::Arc;

use vector_config::configurable_component;

use crate::event::Event;

mod check_fields;
pub(self) mod datadog_search;
pub(crate) mod is_log;
pub(crate) mod is_metric;
pub mod not;
mod vrl;

pub use self::vrl::VrlConfig;
use self::{
    check_fields::{CheckFields, CheckFieldsConfig},
    datadog_search::{DatadogSearchConfig, DatadogSearchRunner},
    is_log::IsLog,
    is_metric::IsMetric,
    not::{Not, NotConfig},
    vrl::Vrl,
};

#[derive(Debug, Clone)]
pub enum Condition {
    /// Negates the result of a nested condition.
    Not(Not),

    /// Matches an event if it is a log.
    IsLog(IsLog),

    /// Matches an event if it is a metric.
    IsMetric(IsMetric),

    /// Matches an event with a [Vector Remap Language](https://vector.dev/docs/reference/vrl) (VRL) [boolean expression](https://vector.dev/docs/reference/vrl#boolean-expressions).
    Vrl(Vrl),

    /// Matches an event against an arbitrary set of predicate/value combinations.
    CheckFields(CheckFields),

    /// Matches an event with a [Datadog Search](https://docs.datadoghq.com/logs/explorer/search_syntax/) query.
    DatadogSearch(DatadogSearchRunner),

    /// Matches an event based on an arbitrary implementation of `Condition`.
    Arbitrary(Arc<dyn Conditional + Send + Sync>),

    /// Matches any event.
    AlwaysPass,

    /// Matches no event.
    AlwaysFail,
}

impl Condition {
    pub fn arbitrary<A>(arb: A) -> Self
    where
        A: Conditional + Send + Sync + 'static,
    {
        Self::Arbitrary(Arc::new(arb))
    }

    /// Checks if a condition is true.
    ///
    /// The event should not be modified, it is only mutable so it can be passed into VRL, but VRL type checking prevents mutation.
    pub(crate) fn check(&self, e: Event) -> (bool, Event) {
        match self {
            Condition::Not(x) => x.check(e),
            Condition::IsLog(x) => x.check(e),
            Condition::IsMetric(x) => x.check(e),
            Condition::Vrl(x) => x.check(e),
            Condition::CheckFields(x) => x.check(e),
            Condition::DatadogSearch(x) => x.check(e),
            Condition::Arbitrary(x) => x.check(e),
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
            Condition::Not(x) => x.check_with_context(e),
            Condition::IsLog(x) => x.check_with_context(e),
            Condition::IsMetric(x) => x.check_with_context(e),
            Condition::Vrl(x) => x.check_with_context(e),
            Condition::CheckFields(x) => x.check_with_context(e),
            Condition::DatadogSearch(x) => x.check_with_context(e),
            Condition::Arbitrary(x) => x.check_with_context(e),
            Condition::AlwaysPass => (Ok(()), e),
            Condition::AlwaysFail => (Ok(()), e),
        }
    }
}

/// An event matching condition.
///
/// Many methods exist for matching for matching events, such as using a VRL expression, a Datadog Search query string,
/// or hard-coded matchers like "must be a metric" or "fields A, B, and C must match these constraints".
///
/// They can specified with an enum-style notation:
///
/// ```toml
/// condition.type = 'check_fields'
/// condition."message.equals" = 'hooray'
/// ```
#[configurable_component]
#[derive(Debug, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ConditionConfig {
    /// Negates the result of a nested condition.
    Not(#[configurable(derived)] NotConfig),

    /// Matches an event if it is a log.
    IsLog,

    /// Matches an event if it is a metric.
    IsMetric,

    /// Matches an event with a [Vector Remap Language](https://vector.dev/docs/reference/vrl) (VRL) [boolean expression](https://vector.dev/docs/reference/vrl#boolean-expressions).
    Vrl(#[configurable(derived)] VrlConfig),

    /// Matches an event against an arbitrary set of predicate/value combinations.
    CheckFields(#[configurable(derived)] CheckFieldsConfig),

    /// Matches an event with a [Datadog Search](https://docs.datadoghq.com/logs/explorer/search_syntax/) query.
    DatadogSearch(#[configurable(derived)] DatadogSearchConfig),

    /// Matches an event based on an arbitrary implementation of `Condition`.
    ///
    /// This is not usable from normal user-based configurations, and only exists as a way to use arbitrary
    /// implementations of `Condition` in components that take `AnyCondition` but are initialized directly, such as the
    /// `kubernetes_logs` source using the `reduce` transform directly.
    #[serde(skip)]
    Arbitrary(#[configurable(derived)] Box<dyn ConditionalConfig>),
}

impl ConditionConfig {
    pub fn arbitrary<A>(arb: A) -> Self
    where
        A: ConditionalConfig + 'static,
    {
        Self::Arbitrary(Box::new(arb))
    }

    pub fn build(&self, enrichment_tables: &enrichment::TableRegistry) -> crate::Result<Condition> {
        match self {
            ConditionConfig::Not(x) => x.build(enrichment_tables),
            ConditionConfig::IsLog => Ok(Condition::IsLog(IsLog::default())),
            ConditionConfig::IsMetric => Ok(Condition::IsMetric(IsMetric::default())),
            ConditionConfig::Vrl(x) => x.build(enrichment_tables),
            ConditionConfig::CheckFields(x) => x.build(enrichment_tables),
            ConditionConfig::DatadogSearch(x) => x.build(enrichment_tables),
            ConditionConfig::Arbitrary(x) => x.build(enrichment_tables),
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
    fn build(&self, enrichment_tables: &enrichment::TableRegistry) -> crate::Result<Condition>;
}

dyn_clone::clone_trait_object!(ConditionalConfig);

/// An event matching condition.
///
/// Many methods exist for matching for matching events, such as using a VRL expression, a Datadog Search query string,
/// or hard-coded matchers like "must be a metric" or "fields A, B, and C must match these constraints".
///
/// As VRL is the most common way to apply conditions to events, this type provides a shortcut to define VRL expressions
/// directly in configuration by passing the VRL expression as a string:
///
/// ```toml
/// condition = '.message == "hooray"'
/// ```
///
/// When other condition types are required, they can specified with an enum-style notation:
///
/// ```toml
/// condition.type = 'check_fields'
/// condition."message.equals" = 'hooray'
/// ```
#[configurable_component]
#[derive(Clone, Debug)]
#[serde(untagged)]
pub enum AnyCondition {
    /// A [Vector Remap Language](https://vector.dev/docs/reference/vrl) (VRL) [boolean expression](https://vector.dev/docs/reference/vrl#boolean-expressions).
    String(#[configurable(transparent)] String),

    /// A fully-specified condition.
    Map(#[configurable(derived)] ConditionConfig),
}

impl AnyCondition {
    pub fn build(&self, enrichment_tables: &enrichment::TableRegistry) -> crate::Result<Condition> {
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
    fn deserialize_anycondition_check_fields() {
        let conf: Test = toml::from_str(indoc! {r#"
            condition.type = "check_fields"
            condition."norg.equals" = "nork"
        "#})
        .unwrap();

        assert_eq!(
            r#"Map(CheckFieldsConfig { predicates: {"norg.equals": "nork"} })"#,
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
            r#"Map(VrlConfig { source: ".nork == true", runtime: Ast })"#,
            format!("{:?}", conf.condition)
        )
    }
}
