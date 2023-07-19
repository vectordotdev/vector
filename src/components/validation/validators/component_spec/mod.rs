use vector_core::event::{Event, Metric, MetricKind};

use crate::components::validation::{
    component_names::*, ComponentType, RunnerMetrics, TestCaseExpectation, TestEvent,
};

use super::{ComponentMetricType, Validator};

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
        component_type: ComponentType,
        expectation: TestCaseExpectation,
        inputs: &[TestEvent],
        outputs: &[Event],
        telemetry_events: &[Event],
        runner_metrics: &RunnerMetrics,
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

        let out = validate_telemetry(component_type, telemetry_events, runner_metrics)?;
        run_out.extend(out);

        Ok(run_out)
    }
}

fn validate_telemetry(
    component_type: ComponentType,
    telemetry_events: &[Event],
    runner_metrics: &RunnerMetrics,
) -> Result<Vec<String>, Vec<String>> {
    let mut out: Vec<String> = Vec::new();
    let mut errs: Vec<String> = Vec::new();

    let metric_types = [
        ComponentMetricType::EventsReceived,
        ComponentMetricType::EventsReceivedBytes,
        ComponentMetricType::ReceivedBytesTotal,
        ComponentMetricType::SentEventsTotal,
        ComponentMetricType::SentEventBytesTotal,
        ComponentMetricType::SentBytesTotal,
        ComponentMetricType::EventsDropped,
    ];

    metric_types.iter().for_each(|metric_type| {
        match validate_metric(
            telemetry_events,
            runner_metrics,
            metric_type,
            component_type,
        ) {
            Err(e) => errs.extend(e),
            Ok(m) => out.extend(m),
        }
    });

    if errs.is_empty() {
        Ok(out)
    } else {
        Err(errs)
    }
}

fn validate_metric(
    telemetry_events: &[Event],
    runner_metrics: &RunnerMetrics,
    metric_type: &ComponentMetricType,
    component_type: ComponentType,
) -> Result<Vec<String>, Vec<String>> {
    let component_name = match component_type {
        ComponentType::Source => TEST_SOURCE_NAME,
        ComponentType::Transform => TEST_TRANSFORM_NAME,
        ComponentType::Sink => TEST_SINK_NAME,
    };

    match metric_type {
        ComponentMetricType::EventsReceived => {
            // The reciprocal metric for events received is events sent,
            // so the expected value is what the input runner sent.
            let expected_events = runner_metrics.sent_events_total;

            compare_actual_to_expected(
                telemetry_events,
                &ComponentMetricType::EventsReceived,
                component_name,
                expected_events,
            )
        }
        ComponentMetricType::EventsReceivedBytes => {
            // The reciprocal metric for received_event_bytes is sent_event_bytes,
            // so the expected value is what the input runner sent.
            let expected_bytes = runner_metrics.sent_event_bytes_total;

            compare_actual_to_expected(
                telemetry_events,
                &ComponentMetricType::EventsReceivedBytes,
                component_name,
                expected_bytes,
            )
        }
        ComponentMetricType::ReceivedBytesTotal => {
            // The reciprocal metric for received_bytes is sent_bytes,
            // so the expected value is what the input runner sent.
            let expected_bytes = if component_type == ComponentType::Sink {
                0 // sinks should not emit this metric
            } else {
                runner_metrics.sent_bytes_total
            };
            compare_actual_to_expected(
                telemetry_events,
                &ComponentMetricType::ReceivedBytesTotal,
                component_name,
                expected_bytes,
            )
        }
        ComponentMetricType::SentEventsTotal => {
            // The reciprocal metric for events sent is events received,
            // so the expected value is what the output runner received.
            let expected_events = runner_metrics.received_events_total;

            compare_actual_to_expected(
                telemetry_events,
                &ComponentMetricType::SentEventsTotal,
                component_name,
                expected_events,
            )
        }
        ComponentMetricType::SentBytesTotal => {
            // The reciprocal metric for sent_bytes is received_bytes,
            // so the expected value is what the output runner received.
            let expected_bytes = if component_type == ComponentType::Source {
                0 // sources should not emit this metric
            } else {
                runner_metrics.received_bytes_total
            };

            compare_actual_to_expected(
                telemetry_events,
                &ComponentMetricType::SentBytesTotal,
                component_name,
                expected_bytes,
            )
        }
        ComponentMetricType::SentEventBytesTotal => {
            // The reciprocal metric for sent_event_bytes is received_event_bytes,
            // so the expected value is what the output runner received.
            let expected_bytes = runner_metrics.received_event_bytes_total;

            compare_actual_to_expected(
                telemetry_events,
                &ComponentMetricType::SentEventBytesTotal,
                component_name,
                expected_bytes,
            )
        }
        ComponentMetricType::EventsDropped => {
            // TODO
            Ok(vec![])
        }
    }
}

fn filter_events_by_metric_and_component<'a>(
    telemetry_events: &'a [Event],
    metric: &ComponentMetricType,
    component_name: &'a str,
) -> Vec<&'a Metric> {
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

    debug!("{}: {} metrics found.", metric.to_string(), metrics.len(),);

    metrics
}

fn sum_counters(
    metric_name: &ComponentMetricType,
    metrics: &[&vector_core::event::Metric],
) -> Result<u64, Vec<String>> {
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
        Ok(sum as u64)
    } else {
        Err(errs)
    }
}

fn compare_actual_to_expected(
    telemetry_events: &[Event],
    metric_type: &ComponentMetricType,
    component_name: &str,
    expected: u64,
) -> Result<Vec<String>, Vec<String>> {
    let mut errs: Vec<String> = Vec::new();

    let metrics =
        filter_events_by_metric_and_component(telemetry_events, metric_type, component_name);

    let actual = sum_counters(metric_type, &metrics)?;

    debug!("{}: expected {}, actual {}.", metric_type, expected, actual,);

    if actual != expected {
        errs.push(format!(
            "{}: expected {}, but received {}",
            metric_type, expected, actual
        ));
    }

    if !errs.is_empty() {
        return Err(errs);
    }

    Ok(vec![format!("{}: {}", metric_type, actual)])
}
