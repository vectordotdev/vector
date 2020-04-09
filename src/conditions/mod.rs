use crate::topology::config::component::ComponentDescription;
use crate::Event;
use inventory;
use serde::{Deserialize, Serialize};

pub mod check_fields;
pub mod is_log;
pub mod is_metric;

pub use check_fields::CheckFieldsConfig;

pub trait Condition: Send + Sync {
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
pub trait ConditionConfig: std::fmt::Debug {
    fn build(&self) -> crate::Result<Box<dyn Condition>>;
}

pub type ConditionDescription = ComponentDescription<Box<dyn ConditionConfig>>;

inventory::collect!(ConditionDescription);

#[derive(Debug, Deserialize, Serialize)]
#[serde(untagged)]
pub enum AnyCondition {
    FromType(Box<dyn ConditionConfig>),
    NoTypeCondition(CheckFieldsConfig),
}

impl AnyCondition {
    pub fn build(&self) -> crate::Result<Box<dyn Condition>> {
        match self {
            Self::FromType(c) => c.build(),
            Self::NoTypeCondition(c) => c.build(),
        }
    }
}
