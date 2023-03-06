use std::fmt::{Display, Formatter};

use bytes::BytesMut;
use vector_core::event::{Event, Metric, MetricKind};
use vector_core::EstimatedJsonEncodedSizeOf;

use crate::components::validation::{
    encode_test_event, ComponentConfiguration, ComponentType, ResourceCodec, TestCaseExpectation,
    TestEvent,
};

use crate::components::validation::runner::config::TEST_SOURCE_NAME;
use crate::sources::Sources;

use super::Validator;

/// Validates that the component meets the requirements of the [Component Specification][component_spec].
///
/// Generally speaking, the Component Specification dictates the expected events and metrics
/// that must be emitted by a component of a specific type. This ensures that not only are
/// metrics emitting the expected telemetry, but that operators can depend on, for example, any
/// source to always emit a specific base set of metrics that are specific to sources, and so on.
///
/// [component_spec]: https://github.com/vectordotdev/vector/blob/master/docs/specs/component.md
#[derive(Default)]
pub struct ComponentSpecValidator;

impl Validator for ComponentSpecValidator {
    fn name(&self) -> &'static str {
        "component_spec"
    }

    fn check_validation(
        &self,
        configuration: ComponentConfiguration,
        component_type: ComponentType,
        expectation: TestCaseExpectation,
        inputs: &[TestEvent],
        outputs: &[Event],
        telemetry_events: &[Event],
    ) -> Result<Vec<String>, Vec<String>> {
        for input in inputs {
            debug!("Validator observed input event: {:?}", input);
        }

        for output in outputs {
            debug!("Validator observed output event: {:?}", output);
        }

        // Validate that the number of inputs/outputs matched the test case expectation.
        //
        // NOTE: This logic currently assumes that one input event leads to, at most, one output
        // event. It also assumes that tests that are marked as expecting to be partially successful
        // should never emit the same number of output events as there are input events.
        match expectation {
            TestCaseExpectation::Success => {
                if inputs.len() != outputs.len() {
                    return Err(vec![format!(
                        "Sent {} inputs but only received {} outputs.",
                        inputs.len(),
                        outputs.len()
                    )]);
                }
            }
            TestCaseExpectation::Failure => {
                if !outputs.is_empty() {
                    return Err(vec![format!(
                        "Received {} outputs but none were expected.",
                        outputs.len()
                    )]);
                }
            }
            TestCaseExpectation::PartialSuccess => {
                if inputs.len() == outputs.len() {
                    return Err(vec![
                        "Received an output event for every input, when only some outputs were expected.".to_string()
                    ]);
                }
            }
        }

        let mut run_out = vec![
            format!(
                "sent {} inputs and received {} outputs",
                inputs.len(),
                outputs.len()
            ),
            format!("received {} telemetry events", telemetry_events.len()),
        ];

        let out = validate_telemetry(
            configuration,
            component_type,
            inputs,
            outputs,
            telemetry_events,
        )?;
        run_out.extend(out);

        Ok(run_out)
    }
}

enum SourceMetrics {
    EventsReceived,
    EventsReceivedBytes,
    ReceivedBytesTotal,
    SentEventsTotal,
    SentEventBytesTotal,
}

impl SourceMetrics {
    const fn name(&self) -> &'static str {
        match self {
            SourceMetrics::EventsReceived => "component_received_events_total",
            SourceMetrics::EventsReceivedBytes => "component_received_event_bytes_total",
            SourceMetrics::ReceivedBytesTotal => "component_received_bytes_total",
            SourceMetrics::SentEventsTotal => "component_sent_events_total",
            SourceMetrics::SentEventBytesTotal => "component_sent_event_bytes_total",
        }
    }

    fn _from_name(name: &str) -> Option<Self> {
        match name {
            "component_received_events_total" => Some(SourceMetrics::EventsReceived),
            "component_received_event_bytes_total" => Some(SourceMetrics::EventsReceivedBytes),
            "component_received_bytes_total" => Some(SourceMetrics::ReceivedBytesTotal),
            "component_sent_events_total" => Some(SourceMetrics::SentEventsTotal),
            "component_sent_event_bytes_total" => Some(SourceMetrics::SentEventBytesTotal),
            _ => None,
        }
    }
}

impl Display for SourceMetrics {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name())
    }
}

fn validate_telemetry(
    configuration: ComponentConfiguration,
    component_type: ComponentType,
    inputs: &[TestEvent],
    outputs: &[Event],
    telemetry_events: &[Event],
) -> Result<Vec<String>, Vec<String>> {
    let mut out: Vec<String> = Vec::new();
    let mut errs: Vec<String> = Vec::new();

    match component_type {
        ComponentType::Source => {
            let validations = [
                validate_component_received_events_total,
                validate_component_received_event_bytes_total,
                validate_component_received_bytes_total,
                validate_component_sent_events_total,
                validate_component_sent_event_bytes_total,
            ];

            for validation in validations.iter() {
                match validation(&configuration, inputs, outputs, telemetry_events) {
                    Err(e) => errs.extend(e),
                    Ok(m) => out.extend(m),
                }
            }
        }
        ComponentType::Sink => {}
        ComponentType::Transform => {}
    }

    if errs.is_empty() {
        Ok(out)
    } else {
        Err(errs)
    }
}

fn filter_events_by_metric_and_component<'a>(
    telemetry_events: &'a [Event],
    metric: SourceMetrics,
    component_name: &'a str,
) -> Result<Vec<&'a Metric>, Vec<String>> {
    let metrics: Vec<&Metric> = telemetry_events
        .iter()
        .flat_map(|e| {
            if let vector_core::event::Event::Metric(m) = e {
                Some(m)
            } else {
                None
            }
        })
        .filter(|&m| {
            if m.name() == metric.to_string() {
                if let Some(tags) = m.tags() {
                    if tags.get("component_name").unwrap_or("") == component_name {
                        return true;
                    }
                }
            }

            false
        })
        .collect();

    debug!("{}: {} metrics found", metric.to_string(), metrics.len(),);

    if metrics.is_empty() {
        return Err(vec![format!("{}: no metrics were emitted.", metric)]);
    }

    Ok(metrics)
}

fn validate_component_received_events_total(
    _configuration: &ComponentConfiguration,
    inputs: &[TestEvent],
    _outputs: &[Event],
    telemetry_events: &[Event],
) -> Result<Vec<String>, Vec<String>> {
    let mut errs: Vec<String> = Vec::new();

    let metrics = filter_events_by_metric_and_component(
        telemetry_events,
        SourceMetrics::EventsReceived,
        TEST_SOURCE_NAME,
    )?;

    let mut events: f64 = 0.0;
    for m in metrics {
        match m.value() {
            vector_core::event::MetricValue::Counter { value } => {
                if let MetricKind::Absolute = m.data().kind {
                    events = *value
                } else {
                    events += value
                }
            }
            _ => errs.push(format!(
                "{}: metric value is not a counter",
                SourceMetrics::EventsReceived,
            )),
        }
    }

    let expected_events = inputs.iter().fold(0, |acc, i| {
        if let TestEvent::Passthrough(_) = i {
            return acc + 1;
        }
        acc
    });

    debug!(
        "{}: {} events, {} expected events",
        SourceMetrics::EventsReceived,
        events,
        expected_events,
    );

    if events != expected_events as f64 {
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
    _configuration: &ComponentConfiguration,
    inputs: &[TestEvent],
    _outputs: &[Event],
    telemetry_events: &[Event],
) -> Result<Vec<String>, Vec<String>> {
    let mut errs: Vec<String> = Vec::new();

    let metrics = filter_events_by_metric_and_component(
        telemetry_events,
        SourceMetrics::EventsReceivedBytes,
        TEST_SOURCE_NAME,
    )?;

    let mut metric_bytes: f64 = 0.0;
    for m in metrics {
        match m.value() {
            vector_core::event::MetricValue::Counter { value } => {
                if let MetricKind::Absolute = m.data().kind {
                    metric_bytes = *value
                } else {
                    metric_bytes += value
                }
            }
            _ => errs.push(format!(
                "{}: metric value is not a counter",
                SourceMetrics::EventsReceivedBytes,
            )),
        }
    }

    let expected_bytes = inputs.iter().fold(0, |acc, i| {
        if let TestEvent::Passthrough(_) = i {
            let size = vec![i.clone().into_event()].estimated_json_encoded_size_of();
            return acc + size;
        }

        // If we don't have a valid event, we'll just add the JSON length of an empty container,
        // like []
        acc + 2
    });

    debug!(
        "{}: {} bytes, {} expected bytes",
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
    configuration: &ComponentConfiguration,
    inputs: &[TestEvent],
    _outputs: &[Event],
    telemetry_events: &[Event],
) -> Result<Vec<String>, Vec<String>> {
    let mut errs: Vec<String> = Vec::new();

    let metrics = filter_events_by_metric_and_component(
        telemetry_events,
        SourceMetrics::ReceivedBytesTotal,
        TEST_SOURCE_NAME,
    )?;

    let mut metric_bytes: f64 = 0.0;
    for m in metrics {
        match m.value() {
            vector_core::event::MetricValue::Counter { value } => {
                if let MetricKind::Absolute = m.data().kind {
                    metric_bytes = *value
                } else {
                    metric_bytes += value
                }
            }
            _ => errs.push(format!(
                "{}: metric value is not a counter",
                SourceMetrics::ReceivedBytesTotal,
            )),
        }
    }

    let mut expected_bytes = 0;

    // TODO: extract this to somewhere else
    if let ComponentConfiguration::Source(Sources::HttpClient(c)) = configuration {
        let mut encoder = ResourceCodec::from(c.get_decoding_config(None)).into_encoder();

        for i in inputs {
            let mut buffer = BytesMut::new();
            encode_test_event(&mut encoder, &mut buffer, i.clone());
            expected_bytes += buffer.len()
        }
    }

    debug!(
        "{}: {} bytes, {} expected bytes",
        SourceMetrics::ReceivedBytesTotal,
        metric_bytes,
        expected_bytes,
    );

    if metric_bytes != expected_bytes as f64 {
        errs.push(format!(
            "{}: expected {} bytes, but received {}",
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
    _configuration: &ComponentConfiguration,
    inputs: &[TestEvent],
    _outputs: &[Event],
    telemetry_events: &[Event],
) -> Result<Vec<String>, Vec<String>> {
    let mut errs: Vec<String> = Vec::new();

    let metrics = filter_events_by_metric_and_component(
        telemetry_events,
        SourceMetrics::SentEventsTotal,
        TEST_SOURCE_NAME,
    )?;

    let mut events: f64 = 0.0;
    for m in metrics {
        match m.value() {
            vector_core::event::MetricValue::Counter { value } => {
                if let MetricKind::Absolute = m.data().kind {
                    events = *value
                } else {
                    events += value
                }
            }
            _ => errs.push(format!(
                "{}: metric value is not a counter",
                SourceMetrics::SentEventsTotal,
            )),
        }
    }

    let expected_events = inputs.iter().fold(0, |acc, i| {
        if let TestEvent::Passthrough(_) = i {
            return acc + 1;
        }
        acc
    });

    debug!(
        "{}: {} events, {} expected events",
        SourceMetrics::SentEventsTotal,
        events,
        expected_events,
    );

    if events != expected_events as f64 {
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
    _configuration: &ComponentConfiguration,
    _inputs: &[TestEvent],
    outputs: &[Event],
    telemetry_events: &[Event],
) -> Result<Vec<String>, Vec<String>> {
    let mut errs: Vec<String> = Vec::new();

    let metrics = filter_events_by_metric_and_component(
        telemetry_events,
        SourceMetrics::SentEventBytesTotal,
        TEST_SOURCE_NAME,
    )?;

    let mut metric_bytes: f64 = 0.0;
    for m in metrics {
        match m.value() {
            vector_core::event::MetricValue::Counter { value } => {
                if let MetricKind::Absolute = m.data().kind {
                    metric_bytes = *value
                } else {
                    metric_bytes += value
                }
            }
            _ => errs.push(format!(
                "{}: metric value is not a counter",
                SourceMetrics::SentEventBytesTotal,
            )),
        }
    }

    let mut expected_bytes = 0;
    for e in outputs {
        expected_bytes += vec![e].estimated_json_encoded_size_of();
    }

    debug!(
        "{}: {} bytes, {} expected bytes",
        SourceMetrics::SentEventBytesTotal,
        metric_bytes,
        expected_bytes,
    );

    if metric_bytes != expected_bytes as f64 {
        errs.push(format!(
            "{}: expected {} bytes, but received {}",
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
