use serde::{Deserialize, Serialize};

use super::{AnyCondition, Condition, ConditionConfig};
use crate::event::Event;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NotConfig(AnyCondition);

impl From<AnyCondition> for NotConfig {
    fn from(value: AnyCondition) -> Self {
        Self(value)
    }
}

#[typetag::serde(name = "not")]
impl ConditionConfig for NotConfig {
    fn build(
        &self,
        enrichment_tables: &enrichment::TableRegistry,
    ) -> crate::Result<Box<dyn Condition>> {
        Ok(Box::new(Not(self.0.build(enrichment_tables)?)))
    }
}

#[derive(Clone)]
struct Not(Box<dyn Condition>);

impl Condition for Not {
    fn check(&self, e: &Event) -> bool {
        !self.0.check(e)
    }

    fn check_with_context(&self, e: &Event) -> Result<(), String> {
        match self.0.check_with_context(e) {
            Ok(()) => Err("event matches inner condition".to_string()),
            Err(_) => Ok(()),
        }
    }
}
