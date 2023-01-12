mod component_spec;
pub use self::component_spec::ComponentSpecValidator;

use vector_core::event::Event;

use super::{ComponentType, TestCaseExpectation, TestEvent};

/// A component validator.
///
/// Validators perform the actual validation logic that, based on the given inputs, determine of the
/// component is valid or not for the given validator.
pub trait Validator {
    /// Gets the unique name of this validator.
    fn name(&self) -> &'static str;

    /// Processes the given set of inputs/outputs, generating the validation results.
    ///
    /// Additionally, all telemetry events received for the component for the validation run are
    /// provided as well.
    fn check_validation(
        &self,
        component_type: ComponentType,
        expectation: TestCaseExpectation,
        inputs: &[TestEvent],
        outputs: &[Event],
        telemetry_events: &[Event],
    ) -> Result<Vec<String>, Vec<String>>;
}

/// Standard component validators.
///
/// This is an helper enum whose variants can trivially converted into a boxed `dyn Validator`
/// implementation, suitable for use with `Runner::add_validator`.
pub enum StandardValidators {
    /// Validates that the component meets the requirements of the [Component Specification][component_spec].
    ///
    /// See [`ComponentSpecValidator`] for more information.
    ///
    /// [component_spec]: https://github.com/vectordotdev/vector/blob/master/docs/specs/component.md
    ComponentSpec,
}

impl From<StandardValidators> for Box<dyn Validator> {
    fn from(sv: StandardValidators) -> Self {
        match sv {
            StandardValidators::ComponentSpec => Box::<ComponentSpecValidator>::default(),
        }
    }
}
