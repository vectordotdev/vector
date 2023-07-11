use std::fmt::{Display, Formatter};

use bytes::BytesMut;
use vector_core::event::{Event, MetricKind};
use vector_core::EstimatedJsonEncodedSizeOf;

use crate::components::validation::{encode_test_event, TestEvent, ValidationConfiguration};

use super::filter_events_by_metric_and_component;

const TEST_SOURCE_NAME: &str = "test_source";

pub enum SourceMetricType {
    EventsReceived,
    EventsReceivedBytes,
    ReceivedBytesTotal,
    SentEventsTotal,
    SentEventBytesTotal,
}

impl SourceMetricType {
    const fn name(&self) -> &'static str {
        match self {
            SourceMetricType::EventsReceived => "component_received_events_total",
            SourceMetricType::EventsReceivedBytes => "component_received_event_bytes_total",
            SourceMetricType::ReceivedBytesTotal => "component_received_bytes_total",
            SourceMetricType::SentEventsTotal => "component_sent_events_total",
            SourceMetricType::SentEventBytesTotal => "component_sent_event_bytes_total",
        }
    }
}

impl Display for SourceMetricType {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name())
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

fn sum_counters(
    metric_name: &SourceMetricType,
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

fn validate_events_total(
    inputs: &[TestEvent],
    telemetry_events: &[Event],
    metric_type: &SourceMetricType,
    passthrough: bool,
) -> Result<Vec<String>, Vec<String>> {
    let mut errs: Vec<String> = Vec::new();

    let metrics =
        filter_events_by_metric_and_component(telemetry_events, metric_type, TEST_SOURCE_NAME);

    let events: i32 = sum_counters(metric_type, &metrics)? as i32;

    let expected_events = inputs.iter().fold(0, |acc, i| {
        if passthrough {
            if let TestEvent::Passthrough(_) = i {
                return acc + 1;
            }
        } else {
            if let TestEvent::Modified { .. } = i {
                return acc + 1;
            }
        }
        acc
    });

    debug!(
        "{}: {} events, {} expected events.",
        metric_type, events, expected_events,
    );

    if events != expected_events {
        errs.push(format!(
            "{}: expected {} events, but received {}",
            metric_type, expected_events, events
        ));
    }

    if !errs.is_empty() {
        return Err(errs);
    }

    Ok(vec![format!("{}: {}", metric_type, events,)])
}

fn validate_bytes_total(
    telemetry_events: &[Event],
    metric_type: &SourceMetricType,
    expected_bytes: usize,
) -> Result<Vec<String>, Vec<String>> {
    let mut errs: Vec<String> = Vec::new();

    let metrics =
        filter_events_by_metric_and_component(telemetry_events, metric_type, TEST_SOURCE_NAME)?;

    let metric_bytes = sum_counters(metric_type, &metrics)?;

    debug!(
        "{}: {} bytes, {} expected bytes.",
        metric_type, metric_bytes, expected_bytes,
    );

    if metric_bytes != expected_bytes as f64 {
        errs.push(format!(
            "{}: expected {} bytes, but received {}",
            metric_type, expected_bytes, metric_bytes
        ));
    }

    if !errs.is_empty() {
        return Err(errs);
    }

    Ok(vec![format!("{}: {}", metric_type, metric_bytes,)])
}

fn validate_component_received_events_total(
    _configuration: &ValidationConfiguration,
    inputs: &[TestEvent],
    _outputs: &[Event],
    telemetry_events: &[Event],
) -> Result<Vec<String>, Vec<String>> {
    validate_events_total(
        inputs,
        telemetry_events,
        &SourceMetricType::EventsReceived,
        true,
    )
}

fn validate_component_received_event_bytes_total(
    _configuration: &ValidationConfiguration,
    inputs: &[TestEvent],
    _outputs: &[Event],
    telemetry_events: &[Event],
) -> Result<Vec<String>, Vec<String>> {
    let expected_bytes = inputs.iter().fold(0, |acc, i| {
        if let TestEvent::Passthrough(e) = i {
            match e {
                Event::Log(log_event) => info!("event bytes total. test event: {:?}", log_event),
                Event::Metric(_) => todo!(),
                Event::Trace(_) => todo!(),
            }
            let size = vec![e.clone()].estimated_json_encoded_size_of();
            return acc + size;
        }

        acc
    });

    validate_bytes_total(
        telemetry_events,
        &SourceMetricType::EventsReceivedBytes,
        expected_bytes,
    )
}

fn validate_component_received_bytes_total(
    configuration: &ValidationConfiguration,
    inputs: &[TestEvent],
    _outputs: &[Event],
    telemetry_events: &[Event],
) -> Result<Vec<String>, Vec<String>> {
    let mut expected_bytes = 0;
    if let Some(c) = &configuration.external_resource {
        let mut encoder = c.codec.into_encoder();
        for i in inputs {
            let event = match i {
                TestEvent::Passthrough(e) => e,
                TestEvent::Modified { modified: _, event } => event,
            };
            match event {
                Event::Log(log_event) => {
                    info!(" received bytes total. test event: {:?}", log_event)
                }
                Event::Metric(_) => todo!(),
                Event::Trace(_) => todo!(),
            }
            let mut buffer = BytesMut::new();
            encode_test_event(&mut encoder, &mut buffer, i.clone());
            expected_bytes += buffer.len()
        }
    }

    validate_bytes_total(
        telemetry_events,
        &SourceMetricType::ReceivedBytesTotal,
        expected_bytes,
    )
}

fn validate_component_sent_events_total(
    _configuration: &ValidationConfiguration,
    inputs: &[TestEvent],
    _outputs: &[Event],
    telemetry_events: &[Event],
) -> Result<Vec<String>, Vec<String>> {
    validate_events_total(
        inputs,
        telemetry_events,
        &SourceMetricType::SentEventsTotal,
        true,
    )
}

fn validate_component_sent_event_bytes_total(
    _configuration: &ValidationConfiguration,
    _inputs: &[TestEvent],
    outputs: &[Event],
    telemetry_events: &[Event],
) -> Result<Vec<String>, Vec<String>> {
    let mut expected_bytes = 0;
    for e in outputs {
        expected_bytes += vec![e].estimated_json_encoded_size_of();
    }

    validate_bytes_total(
        telemetry_events,
        &SourceMetricType::SentEventBytesTotal,
        expected_bytes,
    )
}
