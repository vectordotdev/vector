use crate::{conditions::Conditional, event::Event};

#[derive(Clone, Debug, Default)]
pub struct IsMetric;

impl Conditional for IsMetric {
    fn check(&self, e: Event) -> (bool, Event) {
        (matches!(e, Event::Metric(_)), e)
    }

    fn check_with_context(&self, e: Event) -> (Result<(), String>, Event) {
        let (result, event) = self.check(e);
        if result {
            (Ok(()), event)
        } else {
            (Err("event is not a metric type".to_string()), event)
        }
    }
}

#[cfg(test)]
mod test {
    use super::IsMetric;
    use crate::{
        conditions::Conditional,
        event::{
            metric::{Metric, MetricKind, MetricValue},
            Event,
        },
    };

    #[test]
    fn is_metric_basic() {
        let cond = IsMetric::default();

        assert!(!cond.check(Event::from("just a log")).0);
        assert!(
            cond.check(Event::from(Metric::new(
                "test metric",
                MetricKind::Incremental,
                MetricValue::Counter { value: 1.0 },
            )))
            .0,
        );
    }
}
