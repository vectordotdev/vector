use std::fmt::{Display, Formatter};

use vector_core::event::{Event, Metric};

use crate::components::validation::{ComponentType, TestCaseExpectation, TestEvent};

use crate::components::validation::runner::config::TEST_SOURCE_NAME;

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

        let out = validate_telemetry(component_type, inputs, outputs, telemetry_events)?;
        run_out.extend(out);

        Ok(run_out)
    }
}

fn validate_telemetry(
    component_type: ComponentType,
    inputs: &[TestEvent],
    _outputs: &[Event],
    telemetry_events: &[Event],
) -> Result<Vec<String>, Vec<String>> {
    let mut out: Vec<String> = Vec::new();
    let mut errs: Vec<String> = Vec::new();

    match component_type {
        ComponentType::Source => {
            match validate_component_events_received(inputs, telemetry_events) {
                Err(e) => errs.extend(e),
                Ok(m) => out.extend(m),
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

fn validate_component_events_received(
    inputs: &[TestEvent],
    telemetry_events: &[Event],
) -> Result<Vec<String>, Vec<String>> {
    let mut errs: Vec<String> = Vec::new();

    let mut metrics = Vec::<Metric>::new();
    for t in telemetry_events.iter() {
        if let vector_core::event::Event::Metric(m) = t {
            if m.name() == SourceMetrics::ComponentEventsReceived.to_string() {
                if let Some(tags) = m.tags() {
                    if tags.get("component_name").unwrap_or("") == TEST_SOURCE_NAME {
                        metrics.push(m.clone());
                    }
                }
            }
        }
    }

    if metrics.is_empty() {
        errs.push(format!(
            "{}: no metrics were emitted.",
            SourceMetrics::ComponentEventsReceived,
        ));

        return Err(errs);
    }

    debug!(
        "{}: {} metrics found",
        SourceMetrics::ComponentEventsReceived,
        metrics.len(),
    );

    let mut events: f64 = 0.0;

    for m in metrics {
        match m.value() {
            vector_core::event::MetricValue::Counter { value } => events += value,
            _ => errs.push(format!(
                "{}: metric value is not a counter",
                SourceMetrics::ComponentEventsReceived,
            )),
        }
    }

    if events != inputs.len() as f64 {
        errs.push(format!(
            "{}: expected {} events, but received {}",
            SourceMetrics::ComponentEventsReceived,
            inputs.len(),
            events
        ));
    }

    debug!(
        "{}: {} total events",
        SourceMetrics::ComponentEventsReceived,
        events,
    );

    if errs.is_empty() {
        Ok(vec![format!(
            "{}: {}",
            SourceMetrics::ComponentEventsReceived,
            events,
        )])
    } else {
        Err(errs)
    }
}

enum SourceMetrics {
    ComponentEventsReceived,
}

impl SourceMetrics {
    const fn name(&self) -> &'static str {
        match self {
            SourceMetrics::ComponentEventsReceived => "component_received_events_total",
        }
    }

    fn _from_name(name: &str) -> Option<Self> {
        match name {
            "component_received_events_total" => Some(SourceMetrics::ComponentEventsReceived),
            _ => None,
        }
    }
}

impl Display for SourceMetrics {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name())
    }
}
