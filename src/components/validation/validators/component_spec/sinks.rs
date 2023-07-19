use vector_core::event::Event;

use crate::components::validation::{component_names::TEST_SINK_NAME, RunnerMetrics};

use super::{ComponentMetricType, ComponentMetricValidator};

pub struct SinkComponentMetricValidator;

impl ComponentMetricValidator for SinkComponentMetricValidator {
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
                    TEST_SINK_NAME,
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
                    TEST_SINK_NAME,
                    expected_bytes,
                )
            }
            ComponentMetricType::ReceivedBytesTotal => {
                Self::validate_bytes_total(
                    telemetry_events,
                    &ComponentMetricType::ReceivedBytesTotal,
                    TEST_SINK_NAME,
                    0, // sinks should not emit this metric
                )
            }
            ComponentMetricType::SentEventsTotal => {
                // The reciprocal metric for events sent is events received,
                // so the expected value is what the output runner received.
                let expected_events = runner_metrics.received_events_total;

                Self::validate_events_total(
                    telemetry_events,
                    &ComponentMetricType::SentEventsTotal,
                    TEST_SINK_NAME,
                    expected_events,
                )
            }
            ComponentMetricType::SentBytesTotal => {
                // The reciprocal metric for sent_bytes is received_bytes,
                // so the expected value is what the output runner received.
                let expected_bytes = runner_metrics.received_bytes_total;

                Self::validate_bytes_total(
                    telemetry_events,
                    &ComponentMetricType::SentBytesTotal,
                    TEST_SINK_NAME,
                    expected_bytes,
                )
            }
            ComponentMetricType::SentEventBytesTotal => {
                // The reciprocal metric for sent_event_bytes is received_event_bytes,
                // so the expected value is what the output runner received.
                let expected_bytes = runner_metrics.received_event_bytes_total;

                Self::validate_bytes_total(
                    telemetry_events,
                    &ComponentMetricType::SentEventBytesTotal,
                    TEST_SINK_NAME,
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
