use metrics::{Counter, Label};

use crate::counter;

use super::{Count, CounterName, InternalEvent, InternalEventHandle, RegisterInternalEvent};
use crate::NamedInternalEvent;

pub const INTENTIONAL: bool = true;
pub const UNINTENTIONAL: bool = false;

#[derive(Debug, NamedInternalEvent)]
pub struct ComponentEventsDropped<'a, const INTENTIONAL: bool> {
    pub count: usize,
    pub reason: &'a str,
}

impl<const INTENTIONAL: bool> InternalEvent for ComponentEventsDropped<'_, INTENTIONAL> {
    fn emit(self) {
        self.emit_with_tags([]);
    }
}

impl<'a, const INTENTIONAL: bool> ComponentEventsDropped<'a, INTENTIONAL> {
    /// Emits the discarded events metric with arbitrary additional labels.
    ///
    /// The standard `intentional`, `reason`, and `count` properties are managed by this event and
    /// cannot be overridden by additional labels.
    pub fn emit_with_tags(self, tags: impl IntoIterator<Item = Label>) {
        #[cfg(any(test, feature = "test"))]
        crate::event_test_util::record_internal_event(<Self as super::NamedInternalEvent>::name(
            &self,
        ));

        let count = self.count;
        self.register_with_tags(tags).emit(Count(count));
    }

    fn register_with_tags(
        self,
        tags: impl IntoIterator<Item = Label>,
    ) -> DroppedHandle<'a, INTENTIONAL> {
        let tags = std::iter::once(Label::new(
            "intentional",
            if INTENTIONAL { "true" } else { "false" },
        ))
        .chain(
            tags.into_iter()
                .filter(|tag| !matches!(tag.key(), "intentional" | "reason" | "count")),
        )
        .collect::<Vec<_>>();

        DroppedHandle {
            discarded_events: counter!(CounterName::ComponentDiscardedEventsTotal, tags),
            reason: self.reason,
        }
    }
}

impl<'a, const INTENTIONAL: bool> From<&'a str> for ComponentEventsDropped<'a, INTENTIONAL> {
    fn from(reason: &'a str) -> Self {
        Self { count: 0, reason }
    }
}

// ComponentEventsDropped is the foundation type the `registered_event!` macro
// abstracts over, so we have to implement RegisterInternalEvent by hand here.
impl<'a, const INTENTIONAL: bool> RegisterInternalEvent
    for ComponentEventsDropped<'a, INTENTIONAL>
{
    // ## skip check-validity-events ##
    type Handle = DroppedHandle<'a, INTENTIONAL>;
    fn register(self) -> Self::Handle {
        self.register_with_tags([])
    }
}

#[derive(Clone)]
pub struct DroppedHandle<'a, const INTENDED: bool> {
    discarded_events: Counter,
    reason: &'a str,
}

impl<const INTENDED: bool> InternalEventHandle for DroppedHandle<'_, INTENDED> {
    type Data = Count;
    fn emit(&self, data: Self::Data) {
        let message = "Events dropped";
        if INTENDED {
            debug!(
                message,
                intentional = INTENDED,
                count = data.0,
                reason = self.reason,
            );
        } else {
            error!(
                message,
                intentional = INTENDED,
                count = data.0,
                reason = self.reason,
            );
        }
        self.discarded_events.increment(data.0 as u64);
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex, atomic::Ordering};

    use metrics::{Gauge, Histogram, Key, KeyName, Metadata, Recorder, SharedString, Unit};

    use super::*;

    #[derive(Default)]
    struct TestRecorder {
        counters: Mutex<Vec<(Key, Arc<metrics::atomics::AtomicU64>)>>,
    }

    impl Recorder for TestRecorder {
        fn describe_counter(&self, _: KeyName, _: Option<Unit>, _: SharedString) {}
        fn describe_gauge(&self, _: KeyName, _: Option<Unit>, _: SharedString) {}
        fn describe_histogram(&self, _: KeyName, _: Option<Unit>, _: SharedString) {}

        fn register_counter(&self, key: &Key, _: &Metadata<'_>) -> Counter {
            let value = Arc::new(metrics::atomics::AtomicU64::new(0));
            self.counters
                .lock()
                .unwrap()
                .push((key.clone(), Arc::clone(&value)));
            Counter::from_arc(value)
        }

        fn register_gauge(&self, _: &Key, _: &Metadata<'_>) -> Gauge {
            panic!("unexpected gauge");
        }

        fn register_histogram(&self, _: &Key, _: &Metadata<'_>) -> Histogram {
            panic!("unexpected histogram");
        }
    }

    // Drop two events via ordinary emit, empty tags, a reused handle, and custom tags.
    // Check that each counter totals two, only the last has custom tags, and reserved tags
    // cannot be overridden. Run for both intentional values with an isolated recorder.
    fn check_emission_paths<const INTENTIONAL: bool>() {
        let recorder = TestRecorder::default();
        metrics::with_local_recorder(&recorder, || {
            let event = || ComponentEventsDropped::<INTENTIONAL> {
                count: 2,
                reason: "test",
            };
            super::super::emit(event());
            event().emit_with_tags([]);
            let handle = event().register();
            handle.emit(Count(1));
            handle.emit(Count(1));
            event().emit_with_tags([
                Label::new("group", String::from("alpha")),
                Label::new("custom", "value"),
                Label::new("intentional", "override"),
                Label::new("reason", "override"),
                Label::new("count", "override"),
            ]);
        });

        let counters = recorder.counters.lock().unwrap();
        assert_eq!(counters.len(), 4);
        let intentional = Label::new("intentional", if INTENTIONAL { "true" } else { "false" });
        for (index, (key, count)) in counters.iter().enumerate() {
            assert_eq!(key.name(), "component_discarded_events_total");
            assert_eq!(count.load(Ordering::Relaxed), 2);
            let mut expected = vec![intentional.clone()];
            if index == 3 {
                expected.extend([Label::new("group", "alpha"), Label::new("custom", "value")]);
            }
            assert_eq!(key.labels().cloned().collect::<Vec<_>>(), expected);
        }
        crate::event_test_util::contains_name_once("ComponentEventsDropped").unwrap();
    }

    #[test]
    fn intentional_emission_paths() {
        check_emission_paths::<INTENTIONAL>();
    }

    #[test]
    fn unintentional_emission_paths() {
        check_emission_paths::<UNINTENTIONAL>();
    }
}
