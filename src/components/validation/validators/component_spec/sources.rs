use vector_core::event::Event;

use crate::components::validation::{component_names::TEST_SOURCE_NAME, RunnerMetrics};

use super::{ComponentMetricType, ComponentMetricValidator};

pub struct SourceComponentMetricValidator;

impl ComponentMetricValidator for SourceComponentMetricValidator {
    fn validate_metric(
        telemetry_events: &[Event],
        runner_metrics: &RunnerMetrics,
        metric_type: &ComponentMetricType,
    ) -> Result<Vec<String>, Vec<String>> {
        match metric_type {
            ComponentMetricType::EventsReceived => {
                // The reciprocal metric for events received is events sent,
                // so the expected value is what the input runner sent.
                let expected_events = runner_metrics.sent_events_total;

                Self::validate_events_total(
                    telemetry_events,
                    &ComponentMetricType::EventsReceived,
                    TEST_SOURCE_NAME,
                    expected_events,
                )
            }
            ComponentMetricType::EventsReceivedBytes => {
                // The reciprocal metric for received_event_bytes is sent_event_bytes,
                // so the expected value is what the input runner sent.
                let expected_bytes = runner_metrics.sent_event_bytes_total;

                Self::validate_bytes_total(
                    telemetry_events,
                    &ComponentMetricType::EventsReceivedBytes,
                    TEST_SOURCE_NAME,
                    expected_bytes,
                )
            }
            ComponentMetricType::ReceivedBytesTotal => {
                // The reciprocal metric for received_bytes is sent_bytes,
                // so the expected value is what the input runner sent.
                let expected_bytes = runner_metrics.sent_bytes_total;

                Self::validate_bytes_total(
                    telemetry_events,
                    &ComponentMetricType::ReceivedBytesTotal,
                    TEST_SOURCE_NAME,
                    expected_bytes,
                )
            }
            ComponentMetricType::SentEventsTotal => {
                // The reciprocal metric for events sent is events received,
                // so the expected value is what the output runner received.
                let expected_events = runner_metrics.received_events_total;

                Self::validate_events_total(
                    telemetry_events,
                    &ComponentMetricType::SentEventsTotal,
                    TEST_SOURCE_NAME,
                    expected_events,
                )
            }
            ComponentMetricType::SentBytesTotal => {
                Self::validate_bytes_total(
                    telemetry_events,
                    &ComponentMetricType::SentBytesTotal,
                    TEST_SOURCE_NAME,
                    0, // sources should not emit this metric
                )
            }
            ComponentMetricType::SentEventBytesTotal => {
                // The reciprocal metric for sent_event_bytes is received_event_bytes,
                // so the expected value is what the output runner received.
                let expected_bytes = runner_metrics.received_event_bytes_total;

                Self::validate_bytes_total(
                    telemetry_events,
                    &ComponentMetricType::SentEventBytesTotal,
                    TEST_SOURCE_NAME,
                    expected_bytes,
                )
            }
            ComponentMetricType::EventsDropped => {
                // TODO
                Ok(vec![])
            }
        }
    }
}
