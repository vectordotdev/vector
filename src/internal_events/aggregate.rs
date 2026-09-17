use vector_lib::{
    NamedInternalEvent, counter,
    internal_event::{ComponentEventsDropped, CounterName, INTENTIONAL, InternalEvent},
};

#[derive(Debug, NamedInternalEvent)]
pub struct AggregateEventRecorded;

impl InternalEvent for AggregateEventRecorded {
    fn emit(self) {
        counter!(CounterName::AggregateEventsRecordedTotal).increment(1);
    }
}

#[derive(Debug, NamedInternalEvent)]
pub struct AggregateFlushed;

impl InternalEvent for AggregateFlushed {
    fn emit(self) {
        counter!(CounterName::AggregateFlushesTotal).increment(1);
    }
}

#[derive(Debug, NamedInternalEvent)]
pub struct AggregateUpdateFailed;

impl InternalEvent for AggregateUpdateFailed {
    fn emit(self) {
        counter!(CounterName::AggregateFailedUpdatesTotal).increment(1);
        counter!(CounterName::AggregateFailedUpdates).increment(1);
    }
}

#[derive(Debug, NamedInternalEvent)]
pub struct AggregateEventDropped {
    pub reason: &'static str,
}

impl InternalEvent for AggregateEventDropped {
    fn emit(self) {
        emit!(ComponentEventsDropped::<INTENTIONAL> {
            count: 1,
            reason: self.reason,
        });
    }
}

#[cfg(test)]
mod tests {
    use vector_lib::{
        event::{Metric, MetricValue},
        metrics::Controller,
    };

    use super::*;

    fn capture_metrics(emit: impl FnOnce()) -> Vec<Metric> {
        vector_lib::metrics::init_test();
        let controller = Controller::get().unwrap();
        controller.reset();

        emit();

        controller.capture_metrics()
    }

    fn assert_single_metric<'a>(metrics: &'a [Metric], name: &str) -> &'a Metric {
        let matches = metrics
            .iter()
            .filter(|metric| metric.name() == name)
            .collect::<Vec<_>>();
        assert_eq!(matches.len(), 1, "expected exactly one metric named {name}");
        matches[0]
    }

    #[test]
    fn emits_total_and_legacy_counters() {
        let metrics = capture_metrics(|| AggregateUpdateFailed.emit());

        let total = assert_single_metric(&metrics, "aggregate_failed_updates_total");
        let legacy = assert_single_metric(&metrics, "aggregate_failed_updates");

        for metric in [total, legacy] {
            assert!(
                matches!(metric.value(), MetricValue::Counter { value } if *value == 1.0),
                "expected counter with value 1 for {}",
                metric.name()
            );
        }
    }
}
