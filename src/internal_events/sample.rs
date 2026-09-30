use metrics::Label;
use vector_lib::{
    NamedInternalEvent,
    internal_event::{ComponentEventsDropped, INTENTIONAL, InternalEvent},
};

#[derive(Debug, NamedInternalEvent)]
pub struct SampleEventDiscarded {
    pub group: Option<String>,
    pub include_group_tag: bool,
}

impl InternalEvent for SampleEventDiscarded {
    fn emit(self) {
        let group_tag = self
            .include_group_tag
            .then(|| Label::new("group", self.group.unwrap_or_else(|| "None".to_string())));
        ComponentEventsDropped::<INTENTIONAL> {
            count: 1,
            reason: "Sample discarded.",
        }
        .emit_with_tags(group_tag);
    }
}

#[cfg(test)]
mod tests {
    use serial_test::serial;
    use vector_lib::{event::MetricValue, internal_event::InternalEvent, metrics::Controller};

    use super::SampleEventDiscarded;

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
        for group in ["group-a", "group-b"] {
            SampleEventDiscarded {
                group: Some(group.to_string()),
                include_group_tag: true,
            }
            .emit();
        }
        SampleEventDiscarded {
            group: None,
            include_group_tag: true,
        }
        .emit();

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
        SampleEventDiscarded {
            group: None,
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
