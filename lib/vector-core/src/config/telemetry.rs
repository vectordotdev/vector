use cfg_if::cfg_if;
use vector_common::{json_size::JsonSize, request_metadata::GroupedCountByteSize};
use vector_config::configurable_component;

use crate::EstimatedJsonEncodedSizeOf;

cfg_if! {
    // The telemetry code assumes a process wide singleton. When running `cargo test`,
    // multiple threads might try to read/write this global.
    if #[cfg(any(test, feature = "test"))] {
        use std::sync::{Arc, Mutex};

        thread_local! {
            static TELEMETRY: Arc<Mutex<Option<Telemetry>>> = Arc::new(Mutex::new(None));
        }

        /// Test implementation.
        ///
        /// # Panics
        ///
        /// If deny is set, will panic if telemetry has already been set.
        /// Also, panics if the lock is poisoned.
        pub fn init_telemetry(telemetry: Telemetry, deny_if_set: bool) {
            TELEMETRY.with(|tl| {
                let mut tl = tl.lock().expect("telemetry lock poisoned");
                assert!(!(tl.is_some() && deny_if_set), "Couldn't set telemetry");
                *tl = Some(telemetry);
            });
        }

        /// Test implementation.
        ///
        /// # Panics
        ///
        /// If the lock is poisoned.
         pub fn telemetry() -> Telemetry {
            TELEMETRY.with(|tl| {
               let mut tl = tl.lock().expect("telemetry lock poisoned");
                // For non-test code we return `TELEMETRY_DEFAULT`.
                // For test code, we will instantiate a default instance per thread.
                if tl.is_none() {
                    *tl = Some(Telemetry::default());
                }
                tl.clone().unwrap()
            })
        }
    }
    else {
        use std::sync::{LazyLock, OnceLock};

        static TELEMETRY: OnceLock<Telemetry> = OnceLock::new();
        static TELEMETRY_DEFAULT: LazyLock<Telemetry> = LazyLock::new(Telemetry::default);

        /// Loads the telemetry options from configurations and sets the global options.
        /// Once this is done, configurations can be correctly loaded using configured
        /// log schema defaults.
        ///
        /// # Errors
        ///
        /// This function will fail if the `builder` fails.
        ///
        /// # Panics
        ///
        /// If deny is set, will panic if telemetry has already been set.
        pub fn init_telemetry(telemetry: Telemetry, deny_if_set: bool) {
            assert!(
                !(TELEMETRY.set(telemetry).is_err() && deny_if_set),
                "Couldn't set telemetry"
            );
        }

        /// Returns the telemetry configuration options.
        pub fn telemetry() -> &'static Telemetry {
            TELEMETRY.get().unwrap_or(&TELEMETRY_DEFAULT)
        }
    }
}

/// Sets options for the telemetry that Vector emits.
#[configurable_component]
#[derive(Clone, Debug, Eq, PartialEq)]
#[serde(default)]
pub struct Telemetry {
    pub tags: Tags,

    /// Whether detailed topology event-size and latency telemetry is emitted.
    ///
    /// Disabling this skips event-size estimates for source outputs and topology
    /// routing, reporting zero for their event byte-size metrics. It also omits
    /// source lag and send latency histograms and component latency metrics.
    /// Event counters, routing metadata, and component-specific telemetry remain
    /// enabled. Defaults to `true`.
    pub emit_detailed_metrics: bool,
}

impl Default for Telemetry {
    fn default() -> Self {
        Self {
            tags: Tags::default(),
            emit_detailed_metrics: true,
        }
    }
}

impl Telemetry {
    /// Merge two `Telemetry` instances together.
    pub fn merge(&mut self, other: &Telemetry) {
        self.tags.emit_service = self.tags.emit_service || other.tags.emit_service;
        self.tags.emit_source = self.tags.emit_source || other.tags.emit_source;
        self.emit_detailed_metrics = self.emit_detailed_metrics && other.emit_detailed_metrics;
    }

    /// Returns true if any of the tag options are true.
    pub fn has_tags(&self) -> bool {
        self.tags.emit_service || self.tags.emit_source
    }

    pub fn tags(&self) -> &Tags {
        &self.tags
    }

    /// The variant of `GroupedCountByteSize`
    pub fn create_request_count_byte_size(&self) -> GroupedCountByteSize {
        if self.has_tags() {
            GroupedCountByteSize::new_tagged()
        } else {
            GroupedCountByteSize::new_untagged()
        }
    }

    /// Returns an event's estimated JSON size when byte-size metrics are
    /// enabled, or zero without walking the event when disabled.
    #[inline]
    pub fn event_size<T: EstimatedJsonEncodedSizeOf + ?Sized>(&self, event: &T) -> JsonSize {
        if self.emit_detailed_metrics {
            event.estimated_json_encoded_size_of()
        } else {
            JsonSize::zero()
        }
    }
}

/// Configures whether to emit certain tags
#[configurable_component]
#[derive(Clone, Debug, Eq, PartialEq, Default)]
#[serde(default)]
pub struct Tags {
    /// True if the `service` tag should be emitted
    /// in the `component_received_*` and `component_sent_*`
    /// telemetry.
    pub emit_service: bool,

    /// True if the `source` tag should be emitted
    /// in the `component_received_*` and `component_sent_*`
    /// telemetry.
    pub emit_source: bool,
}

#[cfg(test)]
mod test {
    use super::*;

    struct MustNotBeMeasured;

    impl EstimatedJsonEncodedSizeOf for MustNotBeMeasured {
        fn estimated_json_encoded_size_of(&self) -> JsonSize {
            panic!("event size was measured while metrics were disabled")
        }
    }

    #[test]
    fn partial_telemetry() {
        let toml = r"
            [tags]
            emit_source = true
        ";
        let telemetry = toml::from_str::<Telemetry>(toml).unwrap();
        assert!(telemetry.emit_detailed_metrics);
        assert!(telemetry.tags.emit_source);
    }

    #[test]
    fn event_size_metrics_can_be_disabled_without_walking_event() {
        let telemetry = Telemetry {
            emit_detailed_metrics: false,
            ..Default::default()
        };

        assert_eq!(telemetry.event_size(&MustNotBeMeasured), JsonSize::zero());
    }

    #[test]
    fn detailed_metrics_default_and_explicit_configuration() {
        for input in ["", "emit_detailed_metrics = true"] {
            let telemetry: Telemetry = toml::from_str(input).unwrap();
            assert!(telemetry.emit_detailed_metrics);
            let event = crate::event::LogEvent::from("message");
            assert_eq!(
                telemetry.event_size(&event),
                event.estimated_json_encoded_size_of()
            );
        }
        let telemetry: Telemetry = toml::from_str("emit_detailed_metrics = false").unwrap();
        assert!(!telemetry.emit_detailed_metrics);
        assert_eq!(telemetry.event_size(&MustNotBeMeasured), JsonSize::zero());
    }

    #[test]
    fn merging_telemetry_preserves_disabled_metrics_and_enabled_tags() {
        for left in [false, true] {
            for right in [false, true] {
                let mut telemetry = Telemetry {
                    emit_detailed_metrics: left,
                    ..Default::default()
                };
                let mut other = Telemetry {
                    emit_detailed_metrics: right,
                    ..Default::default()
                };
                other.tags.emit_source = true;
                other.tags.emit_service = true;
                telemetry.merge(&other);
                assert_eq!(telemetry.emit_detailed_metrics, left && right);
                assert!(telemetry.tags.emit_source);
                assert!(telemetry.tags.emit_service);
            }
        }
    }

    #[test]
    fn disabled_detailed_metrics_preserve_event_timestamps() {
        use crate::{
            event::EventArray,
            latency::LatencyRecorder,
            metrics::{self, Controller},
        };
        use std::time::Instant;
        metrics::init_test();
        init_telemetry(
            Telemetry {
                emit_detailed_metrics: false,
                ..Default::default()
            },
            false,
        );
        let previous = Instant::now();
        let mut event = crate::event::LogEvent::from("message");
        event.metadata_mut().set_last_transform_timestamp(previous);
        let mut events = EventArray::from(event);
        LatencyRecorder::new(None)
            .on_send(&mut events, previous + std::time::Duration::from_secs(1));
        for event in events.iter_events() {
            assert_eq!(
                event.as_log().metadata().last_transform_timestamp(),
                Some(previous)
            );
        }
        assert!(
            !Controller::get()
                .unwrap()
                .capture_metrics()
                .iter()
                .any(|metric| {
                    matches!(
                        metric.name(),
                        "component_latency_seconds" | "component_latency_mean_seconds"
                    )
                })
        );
        init_telemetry(Telemetry::default(), false);
    }
}
