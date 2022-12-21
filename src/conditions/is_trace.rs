use crate::event::Event;

pub(crate) const fn check_is_trace(e: Event) -> (bool, Event) {
    (matches!(e, Event::Trace(_)), e)
}

pub(crate) fn check_is_trace_with_context(e: Event) -> (Result<(), String>, Event) {
    let (result, event) = check_is_trace(e);
    if result {
        (Ok(()), event)
    } else {
        (Err("event is not a trace type".to_string()), event)
    }
}

#[cfg(test)]
mod test {
    use super::check_is_trace;
    use crate::event::{
        metric::{Metric, MetricKind, MetricValue},
        Event, LogEvent, TraceEvent,
    };

    #[test]
    fn is_trace_basic() {
        assert!(
            check_is_trace(Event::from(TraceEvent::from(LogEvent::from(
                "just a trace"
            ))))
            .0
        );
        assert!(
            !check_is_trace(Event::from(Metric::new(
                "test metric",
                MetricKind::Incremental,
                MetricValue::Counter { value: 1.0 },
            )))
            .0,
        );
    }
}
