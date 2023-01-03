use vector_core::event::Event;

use crate::components::validation::{ComponentType, TestCaseExpectation, TestEvent};

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
        _component_type: ComponentType,
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
        match expectation {
            TestCaseExpectation::Success => {
                if inputs.len() != outputs.len() {
                    return Err(vec![format!(
                        "sent {} inputs but only received {} outputs",
                        inputs.len(),
                        outputs.len()
                    )]);
                }
            }
            TestCaseExpectation::Failure => {
                if !outputs.is_empty() {
                    return Err(vec![format!(
                        "received {} outputs but none were expected",
                        outputs.len()
                    )]);
                }
            }
            TestCaseExpectation::PartialSuccess => {
                // TODO: This one is a bit loosy goosy because we should really be checking if we
                // got an output event for every valid input event, where, ostensibly, any event
                // that wasn't marked for modification would represent a valid input event.
                //
                // Thinking it through, however, it's not 100% clear at this moment whether or not
                // we'll ever have a test case, with an expectation of partial success, that
                // intentionally modifies one or more input events but where some of those modified
                // events will lead to output events.
                //
                // I might be overthinking it, but we'll leave this as-is for now because it's
                // certainly the simplest check that matches reality.
                if inputs.len() == outputs.len() {
                    return Err(vec![
                        "received an output event for every input, when only some outputs were expected".to_string()
                    ]);
                }
            }
        }

        // TODO: Check for the relevant telemetry events for the given component type.

        Ok(vec![
            format!(
                "sent {} inputs and received {} outputs",
                inputs.len(),
                outputs.len()
            ),
            format!("received {} telemetry events", telemetry_events.len()),
        ])
    }
}
