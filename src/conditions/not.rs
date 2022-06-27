use serde::{Deserialize, Serialize};

use super::{AnyCondition, Condition, ConditionConfig, Conditional};
use crate::event::Event;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct NotConfig(AnyCondition);

impl From<AnyCondition> for NotConfig {
    fn from(value: AnyCondition) -> Self {
        Self(value)
    }
}

#[typetag::serde(name = "not")]
impl ConditionConfig for NotConfig {
    fn build(&self, enrichment_tables: &enrichment::TableRegistry) -> crate::Result<Condition> {
        Ok(Condition::Not(Not(Box::new(
            self.0.build(enrichment_tables)?,
        ))))
    }
}

#[derive(Debug, Clone)]
pub struct Not(Box<Condition>);

impl Conditional for Not {
    fn check(&self, e: Event) -> (bool, Event) {
        let (result, event) = self.0.check(e);
        (!result, event)
    }

    fn check_with_context(&self, e: Event) -> (Result<(), String>, Event) {
        let (result, event) = self.0.check_with_context(e);
        match result {
            Ok(()) => (Err("event matches inner condition".to_string()), event),
            Err(_) => (Ok(()), event),
        }
    }
}
