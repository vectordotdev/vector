use serde::{Deserialize, Serialize};

use crate::{config::component::ComponentDescription, event::Event};

mod check_fields;
pub(self) mod datadog_search;
pub(crate) mod is_log;
pub(crate) mod is_metric;
pub mod not;
mod vrl;

pub use self::vrl::VrlConfig;

#[derive(Debug, Clone)]
pub enum Condition {
    Not(not::Not),
    IsLog(is_log::IsLog),
    IsMetric(is_metric::IsMetric),
    Vrl(vrl::Vrl),
    CheckFields(check_fields::CheckFields),
    DatadogSearch(datadog_search::DatadogSearchRunner),

    // used for benchmarks
    AlwaysPass,
    AlwaysFail,
}

impl Condition {
    pub(crate) const fn is_log() -> Self {
        Self::IsLog(is_log::IsLog {})
    }

    pub(crate) const fn is_metric() -> Self {
        Self::IsMetric(is_metric::IsMetric {})
    }
}

impl Condition {
    pub(crate) fn check(&self, e: &Event) -> bool {
        match self {
            Condition::IsLog(x) => x.check(e),
            Condition::IsMetric(x) => x.check(e),
            Condition::Not(x) => x.check(e),
            Condition::CheckFields(x) => x.check(e),
            Condition::DatadogSearch(x) => x.check(e),
            Condition::Vrl(x) => x.check(e),
            Condition::AlwaysPass => true,
            Condition::AlwaysFail => false,
        }
    }

    /// Provides context for a failure. This is potentially mildly expensive if
    /// it involves string building and so should be avoided in hot paths.
    pub(crate) fn check_with_context(&self, e: &Event) -> Result<(), String> {
        match self {
            Condition::IsLog(x) => x.check_with_context(e),
            Condition::IsMetric(x) => x.check_with_context(e),
            Condition::Not(x) => x.check_with_context(e),
            Condition::CheckFields(x) => x.check_with_context(e),
            Condition::DatadogSearch(x) => x.check_with_context(e),
            Condition::Vrl(x) => x.check_with_context(e),
            Condition::AlwaysPass => Ok(()),
            Condition::AlwaysFail => Ok(()),
        }
    }
}

pub trait Conditional {
    fn check(&self, e: &Event) -> bool;

    /// Provides context for a failure. This is potentially mildly expensive if
    /// it involves string building and so should be avoided in hot paths.
    fn check_with_context(&self, e: &Event) -> Result<(), String> {
        if self.check(e) {
            Ok(())
        } else {
            Err("condition failed".into())
        }
    }
}

#[typetag::serde(tag = "type")]
pub trait ConditionConfig: std::fmt::Debug + Send + Sync + dyn_clone::DynClone {
    fn build(&self, enrichment_tables: &enrichment::TableRegistry) -> crate::Result<Condition>;
}

dyn_clone::clone_trait_object!(ConditionConfig);

type ConditionDescription = ComponentDescription<Box<dyn ConditionConfig>>;

inventory::collect!(ConditionDescription);

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
/// 
/// ## Warning
///
/// It is not valid to use `#[serde(flatten)]` with this type. If you do so, things will almost certainly break.
#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(untagged)]
pub enum AnyCondition {
    String(String),
    Map(Box<dyn ConditionConfig>),
}

impl AnyCondition {
    pub fn build(&self, enrichment_tables: &enrichment::TableRegistry) -> crate::Result<Condition> {
        match self {
            AnyCondition::String(s) => VrlConfig {
                source: s.clone(),
                runtime: Default::default(),
            }
            .build(enrichment_tables),
            AnyCondition::Map(m) => m.build(enrichment_tables),
        }
    }
}

#[cfg(test)]
mod tests {
    use indoc::indoc;

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
