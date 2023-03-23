use std::fmt::{Display, Formatter};

use bytes::BytesMut;
use vector_core::event::{Event, MetricKind};
use vector_core::EstimatedJsonEncodedSizeOf;

use crate::components::validation::validators::component_spec::filter_events_by_metric_and_component;
use crate::components::validation::{encode_test_event, TestEvent, ValidationConfiguration};

use super::filter_events_by_metric_and_component_with_errors;

const TEST_SOURCE_NAME: &str = "test_source";

pub enum SourceMetrics {
    EventsReceived,
    EventsReceivedBytes,
    ReceivedBytesTotal,
    SentEventsTotal,
    SentEventBytesTotal,
    ErrorsTotal,
    EventsDropped,
}

impl SourceMetrics {
    const fn name(&self) -> &'static str {
        match self {
            SourceMetrics::EventsReceived => "component_received_events_total",
            SourceMetrics::EventsReceivedBytes => "component_received_event_bytes_total",
            SourceMetrics::ReceivedBytesTotal => "component_received_bytes_total",
            SourceMetrics::SentEventsTotal => "component_sent_events_total",
            SourceMetrics::SentEventBytesTotal => "component_sent_event_bytes_total",
            SourceMetrics::ErrorsTotal => "component_errors_total",
            SourceMetrics::EventsDropped => "component_discarded_events_total",
        }
    }
}

pub fn validate_sources(
    configuration: &ValidationConfiguration,
    inputs: &[TestEvent],
    outputs: &[Event],
    telemetry_events: &[Event],
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
        validate_component_errors_dropped_total,
    ];

    for v in validations.iter() {
        match v(configuration, inputs, outputs, telemetry_events) {
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

impl Display for SourceMetrics {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name())
    }
}

fn sum_counters(
    metric_name: SourceMetrics,
    metrics: &[&vector_core::event::Metric],
) -> Result<f64, Vec<String>> {
    let mut sum: f64 = 0.0;
    let mut errs = Vec::new();

    for m in metrics {
        match m.value() {
            vector_core::event::MetricValue::Counter { value } => {
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
        Ok(sum)
    } else {
        Err(errs)
    }
}

fn validate_component_received_events_total(
    _configuration: &ValidationConfiguration,
    inputs: &[TestEvent],
    _outputs: &[Event],
    telemetry_events: &[Event],
) -> Result<Vec<String>, Vec<String>> {
    let mut errs: Vec<String> = Vec::new();

    let metrics = filter_events_by_metric_and_component_with_errors(
        telemetry_events,
        &SourceMetrics::EventsReceived,
        TEST_SOURCE_NAME,
    )?;

    let events: i32 = sum_counters(SourceMetrics::EventsReceived, &metrics)? as i32;

    let expected_events = inputs.iter().fold(0, |acc, i| {
        if let TestEvent::Passthrough(_) = i {
            return acc + 1;
        }
        acc
    });

    debug!(
        "{}: {} events, {} expected events.",
        SourceMetrics::EventsReceived,
        events,
        expected_events,
    );

    if events != expected_events {
        errs.push(format!(
            "{}: expected {} events, but received {}",
            SourceMetrics::EventsReceived,
            expected_events,
            events
        ));
    }

    if !errs.is_empty() {
        return Err(errs);
    }

    Ok(vec![format!(
        "{}: {}",
        SourceMetrics::EventsReceived,
        events,
    )])
}

fn validate_component_received_event_bytes_total(
    _configuration: &ValidationConfiguration,
    inputs: &[TestEvent],
    _outputs: &[Event],
    telemetry_events: &[Event],
) -> Result<Vec<String>, Vec<String>> {
    let mut errs: Vec<String> = Vec::new();

    let metrics = filter_events_by_metric_and_component_with_errors(
        telemetry_events,
        &SourceMetrics::EventsReceivedBytes,
        TEST_SOURCE_NAME,
    )?;

    let metric_bytes: f64 = sum_counters(SourceMetrics::EventsReceivedBytes, &metrics)?;

    let expected_bytes = inputs.iter().fold(0, |acc, i| {
        if let TestEvent::Passthrough(_) = i {
            let size = vec![i.clone().into_event()].estimated_json_encoded_size_of();
            return acc + size;
        }

        acc
    });

    debug!(
        "{}: {} bytes, {} expected bytes.",
        SourceMetrics::EventsReceivedBytes,
        metric_bytes,
        expected_bytes,
    );

    if metric_bytes != expected_bytes as f64 {
        errs.push(format!(
            "{}: expected {} bytes, but received {}",
            SourceMetrics::EventsReceivedBytes,
            expected_bytes,
            metric_bytes
        ));
    }

    if !errs.is_empty() {
        return Err(errs);
    }

    Ok(vec![format!(
        "{}: {}",
        SourceMetrics::EventsReceivedBytes,
        metric_bytes,
    )])
}

fn validate_component_received_bytes_total(
    configuration: &ValidationConfiguration,
    inputs: &[TestEvent],
    _outputs: &[Event],
    telemetry_events: &[Event],
) -> Result<Vec<String>, Vec<String>> {
    let mut errs: Vec<String> = Vec::new();

    let metrics = filter_events_by_metric_and_component_with_errors(
        telemetry_events,
        &SourceMetrics::ReceivedBytesTotal,
        TEST_SOURCE_NAME,
    )?;

    let metric_bytes: f64 = sum_counters(SourceMetrics::ReceivedBytesTotal, &metrics)?;

    let mut expected_bytes = 0;
    if let Some(c) = &configuration.external_resource {
        let mut encoder = c.codec.into_encoder();
        for i in inputs {
            let mut buffer = BytesMut::new();
            encode_test_event(&mut encoder, &mut buffer, i.clone());
            expected_bytes += buffer.len()
        }
    }

    debug!(
        "{}: {} bytes, expected at least {} bytes.",
        SourceMetrics::ReceivedBytesTotal,
        metric_bytes,
        expected_bytes,
    );

    // We'll just establish a lower bound because we can't guarantee that the
    // source will receive an exact number of bytes, since we can't synchronize
    // with its internal logic. For example, some sources push or pull metrics
    // on a schedule (http_client).
    if metric_bytes < expected_bytes as f64 {
        errs.push(format!(
            "{}: expected at least {} bytes, but received {}",
            SourceMetrics::ReceivedBytesTotal,
            expected_bytes,
            metric_bytes
        ));
    }

    if !errs.is_empty() {
        return Err(errs);
    }

    Ok(vec![format!(
        "{}: {}",
        SourceMetrics::ReceivedBytesTotal,
        metric_bytes,
    )])
}

fn validate_component_sent_events_total(
    _configuration: &ValidationConfiguration,
    inputs: &[TestEvent],
    _outputs: &[Event],
    telemetry_events: &[Event],
) -> Result<Vec<String>, Vec<String>> {
    let mut errs: Vec<String> = Vec::new();

    let metrics = filter_events_by_metric_and_component_with_errors(
        telemetry_events,
        &SourceMetrics::SentEventsTotal,
        TEST_SOURCE_NAME,
    )?;

    let events = sum_counters(SourceMetrics::SentEventsTotal, &metrics)? as i32;

    let expected_events = inputs.iter().fold(0, |acc, i| {
        if let TestEvent::Passthrough(_) = i {
            return acc + 1;
        }
        acc
    });

    debug!(
        "{}: {} events, {} expected events.",
        SourceMetrics::SentEventsTotal,
        events,
        expected_events,
    );

    if events != expected_events {
        errs.push(format!(
            "{}: expected {} events, but received {}",
            SourceMetrics::SentEventsTotal,
            inputs.len(),
            events
        ));
    }

    if !errs.is_empty() {
        return Err(errs);
    }

    Ok(vec![format!(
        "{}: {}",
        SourceMetrics::SentEventsTotal,
        events,
    )])
}

fn validate_component_sent_event_bytes_total(
    _configuration: &ValidationConfiguration,
    _inputs: &[TestEvent],
    outputs: &[Event],
    telemetry_events: &[Event],
) -> Result<Vec<String>, Vec<String>> {
    let mut errs: Vec<String> = Vec::new();

    let metrics = filter_events_by_metric_and_component_with_errors(
        telemetry_events,
        &SourceMetrics::SentEventBytesTotal,
        TEST_SOURCE_NAME,
    )?;

    let metric_bytes = sum_counters(SourceMetrics::SentEventBytesTotal, &metrics)?;

    let mut expected_bytes = 0;
    for e in outputs {
        expected_bytes += vec![e].estimated_json_encoded_size_of();
    }

    debug!(
        "{}: {} bytes, {} expected bytes.",
        SourceMetrics::SentEventBytesTotal,
        metric_bytes,
        expected_bytes,
    );

    if metric_bytes != expected_bytes as f64 {
        errs.push(format!(
            "{}: expected {} bytes, but received {}.",
            SourceMetrics::SentEventBytesTotal,
            expected_bytes,
            metric_bytes
        ));
    }

    if !errs.is_empty() {
        return Err(errs);
    }

    Ok(vec![format!(
        "{}: {}",
        SourceMetrics::SentEventBytesTotal,
        metric_bytes,
    )])
}
fn validate_component_errors_total(
    _configuration: &ValidationConfiguration,
    inputs: &[TestEvent],
    _outputs: &[Event],
    telemetry_events: &[Event],
) -> Result<Vec<String>, Vec<String>> {
    let mut errs: Vec<String> = Vec::new();

    let metrics = filter_events_by_metric_and_component(
        telemetry_events,
        &SourceMetrics::ErrorsTotal,
        TEST_SOURCE_NAME,
    );

    let errors: i32 = sum_counters(SourceMetrics::ErrorsTotal, &metrics)? as i32;

    let expected_errors: i32 = inputs.iter().fold(0, |acc, i| {
        if let TestEvent::Modified { .. } = i {
            return acc + 1;
        }
        acc
    });

    debug!(
        "{}: {} errors, expected at least {}.",
        SourceMetrics::ErrorsTotal,
        errors,
        expected_errors,
    );

    // Due to limitations in the test harness, we can only check for a lower
    // bound on the expected number of errors. This is because we can't
    // synchronize with the source's internal logic. For example, the http
    // client source might try to pull metrics from our test server right after
    // we've shut it down, registering an error.
    if errors < expected_errors {
        errs.push(format!(
            "{}: expected at least {} errors, but received {}",
            SourceMetrics::ErrorsTotal,
            expected_errors,
            errors
        ));
    }

    if !errs.is_empty() {
        return Err(errs);
    }

    Ok(vec![format!("{}: {}", SourceMetrics::ErrorsTotal, errors,)])
}

fn validate_component_errors_dropped_total(
    _configuration: &ValidationConfiguration,
    inputs: &[TestEvent],
    _outputs: &[Event],
    telemetry_events: &[Event],
) -> Result<Vec<String>, Vec<String>> {
    let mut errs: Vec<String> = Vec::new();

    let metrics = filter_events_by_metric_and_component(
        telemetry_events,
        &SourceMetrics::EventsDropped,
        TEST_SOURCE_NAME,
    );

    let errors: i32 = sum_counters(SourceMetrics::EventsDropped, &metrics)? as i32;

    let expected_errors: i32 = inputs.iter().fold(0, |acc, i| {
        if let TestEvent::Interrupted { .. } = i {
            return acc + 1;
        }
        acc
    });

    debug!(
        "{}: {} errors, expected at least {}",
        SourceMetrics::EventsDropped,
        errors,
        expected_errors,
    );

    if errors > 0 {
        errs.push(format!(
            "{}: expected at least {} errors, but received {}",
            SourceMetrics::EventsDropped,
            expected_errors,
            errors
        ));
    }

    if !errs.is_empty() {
        return Err(errs);
    }

    Ok(vec![format!(
        "{}: {}",
        SourceMetrics::EventsDropped,
        errors,
    )])
}
