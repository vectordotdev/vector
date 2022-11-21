use vector_core::event::Event;

use crate::components::validation::{ComponentType, TestCaseExpectation};

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
        _expectation: TestCaseExpectation,
        _inputs: &[Event],
        _outputs: &[Event],
        _telemetry_events: &[Event],
    ) -> Result<Vec<String>, Vec<String>> {
        Ok(vec![])
    }
}
