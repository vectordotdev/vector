use std::fmt::{Display, Formatter};

use vector_lib::event::{Event, MetricKind};

use crate::components::validation::RunnerMetrics;

use super::filter_events_by_metric_and_component;

const TEST_SOURCE_NAME: &str = "test_source";

pub enum SourceMetricType {
    EventsReceived,
    EventsReceivedBytes,
    ReceivedBytesTotal,
    SentEventsTotal,
    SentEventBytesTotal,
    ErrorsTotal,
}

impl SourceMetricType {
    const fn name(&self) -> &'static str {
        match self {
            SourceMetricType::EventsReceived => "component_received_events_total",
            SourceMetricType::EventsReceivedBytes => "component_received_event_bytes_total",
            SourceMetricType::ReceivedBytesTotal => "component_received_bytes_total",
            SourceMetricType::SentEventsTotal => "component_sent_events_total",
            SourceMetricType::SentEventBytesTotal => "component_sent_event_bytes_total",
            SourceMetricType::ErrorsTotal => "component_errors_total",
        }
    }
}

impl Display for SourceMetricType {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name())
    }
}

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
        validate_component_errors_total,
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

fn sum_counters(
    metric_name: &SourceMetricType,
    metrics: &[&vector_lib::event::Metric],
) -> Result<u64, Vec<String>> {
    let mut sum: f64 = 0.0;
    let mut errs = Vec::new();

    for m in metrics {
        match m.value() {
            vector_lib::event::MetricValue::Counter { value } => {
                if let MetricKind::Absolute = m.data().kind {
                    sum = *value;
                } else {
                    sum += *value;
                }
            }
            _ => errs.push(format!("{}: metric value is not a counter", metric_name,)),
        }
    }

    if errs.is_empty() {
        Ok(sum as u64)
    } else {
        Err(errs)
    }
}

fn validate_events_total(
    telemetry_events: &[Event],
    metric_type: &SourceMetricType,
    expected_events: u64,
) -> Result<Vec<String>, Vec<String>> {
    let mut errs: Vec<String> = Vec::new();

    let metrics =
        filter_events_by_metric_and_component(telemetry_events, metric_type, TEST_SOURCE_NAME);

    let actual_events = sum_counters(metric_type, &metrics)?;

    debug!(
        "{}: {} events, {} expected events.",
        metric_type, actual_events, expected_events,
    );

    if actual_events != expected_events {
        errs.push(format!(
            "{}: expected {} events, but received {}",
            metric_type, expected_events, actual_events
        ));
    }

    if !errs.is_empty() {
        return Err(errs);
    }

    Ok(vec![format!("{}: {}", metric_type, actual_events)])
}

fn validate_bytes_total(
    telemetry_events: &[Event],
    metric_type: &SourceMetricType,
    expected_bytes: u64,
) -> Result<Vec<String>, Vec<String>> {
    let mut errs: Vec<String> = Vec::new();

    let metrics =
        filter_events_by_metric_and_component(telemetry_events, metric_type, TEST_SOURCE_NAME);

    let actual_bytes = sum_counters(metric_type, &metrics)?;

    debug!(
        "{}: {} bytes, {} expected bytes.",
        metric_type, actual_bytes, expected_bytes,
    );

    if actual_bytes != expected_bytes {
        errs.push(format!(
            "{}: expected {} bytes, but received {}",
            metric_type, expected_bytes, actual_bytes
        ));
    }

    if !errs.is_empty() {
        return Err(errs);
    }

    Ok(vec![format!("{}: {}", metric_type, actual_bytes)])
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
        &SourceMetricType::EventsReceived,
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
        &SourceMetricType::EventsReceivedBytes,
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
        &SourceMetricType::ReceivedBytesTotal,
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
        &SourceMetricType::SentEventsTotal,
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
        &SourceMetricType::SentEventBytesTotal,
        expected_bytes,
    )
}

fn validate_component_errors_total(
    telemetry_events: &[Event],
    runner_metrics: &RunnerMetrics,
) -> Result<Vec<String>, Vec<String>> {
    validate_events_total(
        telemetry_events,
        &SourceMetricType::ErrorsTotal,
        runner_metrics.errors_total,
    )
}
