use vector_lib::{
    NamedInternalEvent, counter,
    internal_event::{ComponentEventsDropped, CounterName, INTENTIONAL, InternalEvent},
};

#[derive(Debug, NamedInternalEvent)]
pub(crate) struct ThrottleEventDiscarded {
    pub key: String,
    pub emit_events_discarded_per_key: bool,
    pub include_group_tag: bool,
}

impl InternalEvent for ThrottleEventDiscarded {
    fn emit(self) {
        let message = "Rate limit exceeded.";

        debug!(message, key = %self.key);
        if self.emit_events_discarded_per_key {
            counter!(CounterName::EventsDiscardedTotal, "key" => self.key.clone()).increment(1); // Deprecated.
        }

        if self.include_group_tag {
            counter!(
                CounterName::ComponentDiscardedEventsTotal,
                "intentional" => "true",
                "group" => self.key,
            )
            .increment(1);
        } else {
            emit!(ComponentEventsDropped::<INTENTIONAL> {
                count: 1,
                reason: message
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use serial_test::serial;
    use vector_lib::{event::MetricValue, internal_event::InternalEvent, metrics::Controller};

    use super::ThrottleEventDiscarded;

    fn discarded_events_counter(tags: &[(&str, &str)]) -> Option<f64> {
        Controller::get()
            .expect("metrics controller initialized")
            .capture_metrics()
            .into_iter()
            .find(|metric| {
                metric.name() == "component_discarded_events_total"
                    && tags.iter().all(|(key, value)| {
                        metric
                            .tags()
                            .is_some_and(|tags| tags.get(key) == Some(*value))
                    })
            })
            .map(|metric| match metric.value() {
                MetricValue::Counter { value } => *value,
                other => panic!("expected counter, got {other:?}"),
            })
    }

    #[test]
    #[serial]
    fn emits_component_discarded_events_with_group_tag() {
        vector_lib::metrics::init_test();
        Controller::get()
            .expect("metrics controller initialized")
            .reset();

        for key in ["group-a", "group-b", "None"] {
            ThrottleEventDiscarded {
                key: key.to_string(),
                emit_events_discarded_per_key: false,
                include_group_tag: true,
            }
            .emit();
        }

        for group in ["group-a", "group-b", "None"] {
            assert_eq!(
                discarded_events_counter(&[("intentional", "true"), ("group", group)]),
                Some(1.0)
            );
        }
    }

    #[test]
    #[serial]
    fn emits_component_discarded_events_without_group_tag_by_default() {
        vector_lib::metrics::init_test();
        Controller::get()
            .expect("metrics controller initialized")
            .reset();

        ThrottleEventDiscarded {
            key: "group-a".to_string(),
            emit_events_discarded_per_key: false,
            include_group_tag: false,
        }
        .emit();

        assert_eq!(
            discarded_events_counter(&[("intentional", "true")]),
            Some(1.0)
        );
        assert_eq!(discarded_events_counter(&[("group", "group-a")]), None);
    }
}
