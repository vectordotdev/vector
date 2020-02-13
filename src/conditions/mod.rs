use crate::topology::config::component::ComponentDescription;
use crate::Event;
use inventory;

pub mod check_fields;

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
