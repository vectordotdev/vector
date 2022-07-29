use serde::{Deserialize, Serialize};

use super::{AnyCondition, Condition, ConditionConfig, Conditional};
use crate::event::{Event, LogEvent, Metric, TraceEvent};

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
    fn check_log(&self, log: LogEvent) -> (bool, LogEvent) {
        let (result, event) = self.0.check_log(log);
        (!result, event)
    }

    fn check_metric(&self, metric: Metric) -> (bool, Metric) {
        let (result, event) = self.0.check_metric(metric);
        (!result, event)
    }

    fn check_trace(&self, trace: TraceEvent) -> (bool, TraceEvent) {
        let (result, event) = self.0.check_trace(trace);
        (!result, event)
    }

    fn check_with_context(&self, event: Event) -> (Result<(), String>, Event) {
        let (result, event) = self.0.check_with_context(event);
        match result {
            Ok(()) => (Err("event matches inner condition".to_string()), event),
            Err(_) => (Ok(()), event),
        }
    }
}
