use serde::{Deserialize, Serialize};

use crate::event::Event;
use std::collections::BTreeMap;

// pub mod check_fields;
// pub mod datadog_search;
pub mod is_log;
pub mod is_metric;
pub mod not;
pub mod vrl;

//pub use check_fields::CheckFieldsConfig;

//pub use self::vrl::VrlConfig;

#[derive(Debug, Clone)]
pub enum Condition {
    IsLog(is_log::IsLog),
    IsMetric(is_metric::IsMetric),
    Not(not::Not),
    // CheckFields(check_fields::CheckFields),
    // DatadogSearch(datadog_search::DatadogSearchRunner),
    Vrl(vrl::Vrl),
}

impl Condition {
    pub(crate) fn check(&self, e: &Event) -> bool {
        match self {
            Condition::IsLog(x) => x.check(e),
            Condition::IsMetric(x) => x.check(e),
            Condition::Not(x) => x.check(e),
            // Condition::CheckFields(x) => x.check(e),
            // Condition::DatadogSearch(x) => x.check(e),
            Condition::Vrl(x) => x.check(e),
        }
    }

    /// Provides context for a failure. This is potentially mildly expensive if
    /// it involves string building and so should be avoided in hot paths.
    pub(crate) fn check_with_context(&self, e: &Event) -> Result<(), String> {
        match self {
            Condition::IsLog(x) => x.check_with_context(e),
            Condition::IsMetric(x) => x.check_with_context(e),
            Condition::Not(x) => x.check_with_context(e),
            // Condition::CheckFields(x) => x.check_with_context(e),
            // Condition::DatadogSearch(x) => x.check_with_context(e),
            Condition::Vrl(x) => x.check_with_context(e),
        }
    }
}

pub(crate) trait Conditional {
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

//dyn_clone::clone_trait_object!(Condition);

// #[typetag::serde(tag = "type")]
// pub trait ConditionConfig: std::fmt::Debug + Send + Sync + dyn_clone::DynClone {
//     fn build(&self, enrichment_tables: &enrichment::TableRegistry) -> crate::Result<Condition>;
// }

//dyn_clone::clone_trait_object!(ConditionConfig);

// pub type ConditionDescription = ComponentDescription<Box<dyn ConditionConfig>>;

// inventory::collect!(ConditionDescription);

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum PlainVariant {
    IsLog,
    IsMetric,
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum CheckFieldsVariant {
    CheckFields,
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum VrlVariant {
    Vrl,
}

/// A condition can either be a raw string such as
/// `condition = '.message == "hooray"'`.
/// In this case it is turned into a VRL condition.
/// Otherwise it is a condition such as:
///
/// condition.type = 'check_fields'
/// condition."message.equals" = 'hooray'
///
///
/// It is important to note that because the way this is
/// structured, it is wrong to flatten a field that contains
/// an AnyCondition:
///
/// #[serde(flatten)]
/// condition: AnyCondition,
///
/// This will result in an error when serializing to json
/// which we need to do when determining which transforms have changed
/// when a config is reloaded.
#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
#[serde(untagged)]
#[serde(rename_all = "snake_case")]
pub enum AnyCondition {
    BareVrl(String),
    Plain {
        #[serde(rename = "type")]
        kind: PlainVariant,
    },
    CheckFields {
        #[serde(rename = "type")]
        kind: CheckFieldsVariant,
        #[serde(flatten)]
        fields: BTreeMap<String, String>,
    },
    Vrl {
        #[serde(rename = "type")]
        kind: VrlVariant,
        source: String,
    },
}

impl AnyCondition {
    pub fn build(
        &self,
        _enrichment_tables: &enrichment::TableRegistry,
    ) -> crate::Result<Condition> {
        unimplemented!()
        // match self {
        //     AnyCondition::IsLog => Ok(Condition::IsLog(is_log::IsLog::default())),
        //     AnyCondition::IsMetric => Ok(Condition::IsMetric(is_metric::IsMetric::default())),
        //     //AnyCondition::String(s) => VrlConfig { source: s.clone() }.build(enrichment_tables),
        //     //            AnyCondition::Map(m) => m.build(enrichment_tables),
        // }
    }
}

#[cfg(test)]
mod tests {
    use crate::conditions::{AnyCondition, CheckFieldsVariant, PlainVariant, VrlVariant};
    use indoc::indoc;
    use serde::Deserialize;
    use std::collections::BTreeMap;

    #[derive(Deserialize, Debug)]
    struct Test {
        condition: AnyCondition,
    }

    #[test]
    fn deserialize_is_log() {
        let conf: Test = toml::from_str(r#"condition.type = "is_log""#).unwrap();
        assert_eq!(
            AnyCondition::Plain {
                kind: PlainVariant::IsLog
            },
            conf.condition
        )
    }

    #[test]
    fn deserialize_is_metric() {
        let conf: Test = toml::from_str(r#"condition.type = "is_metric""#).unwrap();
        assert_eq!(
            AnyCondition::Plain {
                kind: PlainVariant::IsMetric
            },
            conf.condition
        )
    }

    #[test]
    fn deserialize_default() {
        let conf: Test = toml::from_str(r#"condition = ".nork == false""#).unwrap();
        assert_eq!(
            AnyCondition::BareVrl(r#".nork == false"#.to_string()),
            conf.condition
        )
    }

    #[test]
    fn deserialize_anycondition_check_fields() {
        let conf: Test = toml::from_str(indoc! {r#"
            condition.type = "check_fields"
            condition."norg.equals" = "nork"
            condition."foobar" = "raboof"
        "#})
        .unwrap();

        let mut expected_fields: BTreeMap<String, String> = BTreeMap::new();
        expected_fields.insert("norg.equals".to_string(), "nork".to_string());
        expected_fields.insert("foobar".to_string(), "raboof".to_string());

        assert_eq!(
            AnyCondition::CheckFields {
                kind: CheckFieldsVariant::CheckFields,
                fields: expected_fields
            },
            conf.condition
        )
    }

    #[test]
    fn deserialize_anycondition_vrl() {
        let conf: Test = toml::from_str(indoc! {r#"
            condition.type = "vrl"
            condition.source = ".nork == true"
        "#})
        .unwrap();

        assert_eq!(
            AnyCondition::Vrl {
                kind: VrlVariant::Vrl,
                source: ".nork == true".to_string(),
            },
            conf.condition
        )
    }
}
