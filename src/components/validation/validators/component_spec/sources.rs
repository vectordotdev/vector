use vector_core::event::Event;

use crate::components::validation::{component_names::TEST_SOURCE_NAME, RunnerMetrics};

use super::{validate_bytes_total, validate_events_total, ComponentMetricType};

pub fn validate_sources(
    telemetry_events: &[Event],
    runner_metrics: &RunnerMetrics,
) -> Result<Vec<String>, Vec<String>> {
    let mut out: Vec<String> = Vec::new();
    let mut errs: Vec<String> = Vec::new();

    let validations = [
        validate_component_received_events_total,
        validate_component_received_event_bytes_total,
        validate_component_received_bytes_total,
        validate_component_sent_events_total,
        validate_component_sent_event_bytes_total,
    ];

    for v in validations.iter() {
        match v(telemetry_events, runner_metrics) {
            Err(e) => errs.extend(e),
            Ok(m) => out.extend(m),
        }
    }

    if errs.is_empty() {
        Ok(out)
    } else {
        Err(errs)
    }
}

fn validate_component_received_events_total(
    telemetry_events: &[Event],
    runner_metrics: &RunnerMetrics,
) -> Result<Vec<String>, Vec<String>> {
    // The reciprocal metric for events received is events sent,
    // so the expected value is what the input runner sent.
    let expected_events = runner_metrics.sent_events_total;

    validate_events_total(
        telemetry_events,
        &ComponentMetricType::EventsReceived,
        TEST_SOURCE_NAME,
        expected_events,
    )
}

fn validate_component_received_event_bytes_total(
    telemetry_events: &[Event],
    runner_metrics: &RunnerMetrics,
) -> Result<Vec<String>, Vec<String>> {
    // The reciprocal metric for received_event_bytes is sent_event_bytes,
    // so the expected value is what the input runner sent.
    let expected_bytes = runner_metrics.sent_event_bytes_total;

    validate_bytes_total(
        telemetry_events,
        &ComponentMetricType::EventsReceivedBytes,
        TEST_SOURCE_NAME,
        expected_bytes,
    )
}

fn validate_component_received_bytes_total(
    telemetry_events: &[Event],
    runner_metrics: &RunnerMetrics,
) -> Result<Vec<String>, Vec<String>> {
    // The reciprocal metric for received_bytes is sent_bytes,
    // so the expected value is what the input runner sent.
    let expected_bytes = runner_metrics.sent_bytes_total;

    validate_bytes_total(
        telemetry_events,
        &ComponentMetricType::ReceivedBytesTotal,
        TEST_SOURCE_NAME,
        expected_bytes,
    )
}

fn validate_component_sent_events_total(
    telemetry_events: &[Event],
    runner_metrics: &RunnerMetrics,
) -> Result<Vec<String>, Vec<String>> {
    // The reciprocal metric for events sent is events received,
    // so the expected value is what the output runner received.
    let expected_events = runner_metrics.received_events_total;

    validate_events_total(
        telemetry_events,
        &ComponentMetricType::SentEventsTotal,
        TEST_SOURCE_NAME,
        expected_events,
    )
}

fn validate_component_sent_event_bytes_total(
    telemetry_events: &[Event],
    runner_metrics: &RunnerMetrics,
) -> Result<Vec<String>, Vec<String>> {
    // The reciprocal metric for sent_event_bytes is received_event_bytes,
    // so the expected value is what the output runner received.
    let expected_bytes = runner_metrics.received_event_bytes_total;

    validate_bytes_total(
        telemetry_events,
        &ComponentMetricType::SentEventBytesTotal,
        TEST_SOURCE_NAME,
        expected_bytes,
    )
}
