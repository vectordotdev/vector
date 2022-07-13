use crate::event::Event;

pub(crate) const fn check_is_metric(e: Event) -> (bool, Event) {
    (matches!(e, Event::Metric(_)), e)
}

pub(crate) fn check_is_metric_with_context(e: Event) -> (Result<(), String>, Event) {
    let (result, event) = check_is_metric(e);
    if result {
        (Ok(()), event)
    } else {
        (Err("event is not a metric type".to_string()), event)
    }
}

#[cfg(test)]
mod test {
    use super::check_is_metric;
    use crate::event::{
        metric::{Metric, MetricKind, MetricValue},
        Event, LogEvent,
    };

    #[test]
    fn is_metric_basic() {
        assert!(!check_is_metric(Event::from(LogEvent::from("just a log"))).0);
        assert!(
            check_is_metric(Event::from(Metric::new(
                "test metric",
                MetricKind::Incremental,
                MetricValue::Counter { value: 1.0 },
            )))
            .0,
        );
    }
}
