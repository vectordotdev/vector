use serde::{Deserialize, Serialize};

use crate::{
    conditions::{Condition, ConditionConfig, ConditionDescription, Conditional},
    event::{Event, LogEvent, Metric, TraceEvent},
};

//------------------------------------------------------------------------------

#[derive(Deserialize, Serialize, Debug, Default, Clone)]
pub(crate) struct IsLogConfig {}

inventory::submit! {
    ConditionDescription::new::<IsLogConfig>("is_log")
}

impl_generate_config_from_default!(IsLogConfig);

#[typetag::serde(name = "is_log")]
impl ConditionConfig for IsLogConfig {
    fn build(&self, _enrichment_tables: &enrichment::TableRegistry) -> crate::Result<Condition> {
        Ok(Condition::is_log())
    }
}

//------------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct IsLog {}

impl Conditional for IsLog {
    fn check_log(&self, log: LogEvent) -> (bool, LogEvent) {
        (true, log)
    }

    fn check_metric(&self, metric: Metric) -> (bool, Metric) {
        (false, metric)
    }

    fn check_trace(&self, trace: TraceEvent) -> (bool, TraceEvent) {
        (false, trace)
    }

    fn check_with_context(&self, event: Event) -> (Result<(), String>, Event) {
        let (result, event) = self.check(event);
        if result {
            (Ok(()), event)
        } else {
            (Err("event is not a log type".to_string()), event)
        }
    }
}

//------------------------------------------------------------------------------

#[cfg(test)]
mod test {
    use super::*;
    use crate::event::{
        metric::{Metric, MetricKind, MetricValue},
        Event,
    };

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<IsLogConfig>();
    }

    #[test]
    fn is_log_basic() {
        let cond = IsLogConfig {}.build(&Default::default()).unwrap();

        assert!(cond.check(Event::from("just a log")).0);
        assert!(
            !cond
                .check(Event::from(Metric::new(
                    "test metric",
                    MetricKind::Incremental,
                    MetricValue::Counter { value: 1.0 },
                )))
                .0,
        );
    }
}
