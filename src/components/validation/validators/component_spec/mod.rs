mod sinks;
mod sources;

use vector_core::event::{Event, Metric, MetricKind};

use crate::components::validation::{ComponentType, RunnerMetrics, TestCaseExpectation, TestEvent};

use super::{ComponentMetricType, Validator};

use self::sinks::validate_sinks;
use self::sources::validate_sources;

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

    match component_type {
        ComponentType::Source => {
            let result = validate_sources(telemetry_events, runner_metrics);
            match result {
                Ok(o) => out.extend(o),
                Err(e) => errs.extend(e),
            }
        }
        ComponentType::Sink => {
            let result = validate_sinks(telemetry_events, runner_metrics);
            match result {
                Ok(o) => out.extend(o),
                Err(e) => errs.extend(e),
            }
        }
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

fn validate_events_total(
    telemetry_events: &[Event],
    metric_type: &ComponentMetricType,
    component_name: &str,
    expected_events: u64,
) -> Result<Vec<String>, Vec<String>> {
    let mut errs: Vec<String> = Vec::new();

    let metrics =
        filter_events_by_metric_and_component(telemetry_events, metric_type, component_name);

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
    metric_type: &ComponentMetricType,
    component_name: &str,
    expected_bytes: u64,
) -> Result<Vec<String>, Vec<String>> {
    let mut errs: Vec<String> = Vec::new();

    let metrics =
        filter_events_by_metric_and_component(telemetry_events, metric_type, component_name);

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
