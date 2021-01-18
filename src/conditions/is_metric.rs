use serde::{Deserialize, Serialize};

use crate::{
    conditions::{Condition, ConditionConfig, ConditionDescription},
    Event,
};

//------------------------------------------------------------------------------

#[derive(Deserialize, Serialize, Debug, Default, Clone)]
pub struct IsMetricConfig {}

inventory::submit! {
    ConditionDescription::new::<IsMetricConfig>("is_metric")
}

impl_generate_config_from_default!(IsMetricConfig);

#[typetag::serde(name = "is_metric")]
impl ConditionConfig for IsMetricConfig {
    fn build(&self) -> crate::Result<Box<dyn Condition>> {
        Ok(Box::new(IsMetric {}))
    }
}

//------------------------------------------------------------------------------

#[derive(Clone)]
pub struct IsMetric {}

impl Condition for IsMetric {
    fn check(&self, e: &Event) -> bool {
        matches!(e, Event::Metric(_))
    }

    fn check_with_context(&self, e: &Event) -> Result<(), String> {
        if self.check(e) {
            Ok(())
        } else {
            Err("event is not a metric type".to_string())
        }
    }
}

//------------------------------------------------------------------------------

#[cfg(test)]
mod test {
    use super::*;
    use crate::{
        event::metric::{Metric, MetricKind, MetricValue},
        Event,
    };

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<IsMetricConfig>();
    }

    #[test]
    fn is_metric_basic() {
        let cond = IsMetricConfig {}.build().unwrap();

        assert_eq!(cond.check(&Event::from("just a log")), false);
        assert_eq!(
            cond.check(&Event::from(Metric::new(
                "test metric".to_string(),
                None,
                None,
                None,
                MetricKind::Incremental,
                MetricValue::Counter { value: 1.0 },
            ))),
            true
        );
    }
}
