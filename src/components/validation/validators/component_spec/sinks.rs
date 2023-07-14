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
                // Since the runner is on the same "side" of the topology as a sink is,
                // the expected value is what the input runner received.
                // let expected_events = runner_metrics.received_events_total;
                let expected_events = runner_metrics.sent_events_total;

                Self::validate_events_total(
                    telemetry_events,
                    &ComponentMetricType::EventsReceived,
                    TEST_SINK_NAME,
                    expected_events,
                )
            }
            ComponentMetricType::EventsReceivedBytes => {
                // Since the runner is on the same "side" of the topology as a sink is,
                // the expected value is what the input runner received.
                let expected_bytes = runner_metrics.received_event_bytes_total;

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
                // Since the runner is on the same "side" of the topology as a sink is,
                // the expected value is what the input runner sent.
                let expected_events = runner_metrics.received_events_total;

                Self::validate_events_total(
                    telemetry_events,
                    &ComponentMetricType::SentEventsTotal,
                    TEST_SINK_NAME,
                    expected_events,
                )
            }
            ComponentMetricType::SentBytesTotal => {
                // Since the runner is on the same "side" of the topology as a sink is,
                // the expected value is what the input runner sent.
                let expected_bytes = runner_metrics.received_bytes_total;

                Self::validate_bytes_total(
                    telemetry_events,
                    &ComponentMetricType::SentBytesTotal,
                    TEST_SINK_NAME,
                    expected_bytes,
                )
            }
            ComponentMetricType::SentEventBytesTotal => {
                // Since the runner is on the same "side" of the topology as a sink is,
                // the expected value is what the input runner sent.
                let expected_bytes = 0;

                // TODO: see TODO in resources/http.rs for idea on how to get this value

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
