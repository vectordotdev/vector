use crate::topology::config::component::ComponentDescription;
use crate::Event;
use inventory;

pub mod check_fields;

pub trait Condition: Send + Sync {
    fn check(&self, e: &Event) -> bool; // TODO: Add method that provides fail context? -> Result<(), String>
}

#[typetag::serde(tag = "type")]
pub trait ConditionConfig: std::fmt::Debug {
    fn build(&self) -> crate::Result<Box<dyn Condition>>;
}

pub type ConditionDescription = ComponentDescription<Box<dyn ConditionConfig>>;

inventory::collect!(ConditionDescription);
