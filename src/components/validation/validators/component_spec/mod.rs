mod sources;

use vector_core::event::{Event, Metric};

use crate::components::validation::{
    ComponentType, TestCaseExpectation, TestEvent, ValidationConfiguration,
};

use super::Validator;

use self::sources::{validate_sources, SourceMetrics};

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
        configuration: ValidationConfiguration,
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

fn validate_telemetry(
    configuration: ValidationConfiguration,
    component_type: ComponentType,
    inputs: &[TestEvent],
    outputs: &[Event],
    telemetry_events: &[Event],
) -> Result<Vec<String>, Vec<String>> {
    let mut out: Vec<String> = Vec::new();
    let mut errs: Vec<String> = Vec::new();

    match component_type {
        ComponentType::Source => {
            let result = validate_sources(&configuration, inputs, outputs, telemetry_events);
            match result {
                Ok(o) => out.extend(o),
                Err(e) => errs.extend(e),
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

    debug!("{}: {} metrics found.", metric.to_string(), metrics.len(),);

    if metrics.is_empty() {
        return Err(vec![format!("{}: no metrics were emitted.", metric)]);
    }

    Ok(metrics)
}
