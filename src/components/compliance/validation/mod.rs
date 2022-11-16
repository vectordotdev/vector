mod component_spec;
pub use self::component_spec::ComponentSpecValidator;

use vector_core::event::Event;

use super::ComponentType;

/// A component validator.
///
/// Validators perform the actual validation steps, based on component being run for validation, and
/// collect all of the necessary data to do so.
///
/// Validators get access to the input events, and the output events, but are otherwise flexible and
/// can collect whatever data is necessary to complete the validation, driven by pre/post hooks that
/// are called prior to and after a component validation run is complete.
pub trait Validator {
    /// Gets the unique name of this validator.
    fn name(&self) -> &'static str;

    /// Runs the pre-hook logic for this validator.
    ///
    /// This hook allows validator implementations to execute any operations, collect any data, and
    /// so on, before the component is actually run for validation.
    ///
    /// The given `inputs` are the same that will be sent to the component being validated.
    fn run_pre_hook(&mut self, _component_type: ComponentType, _inputs: &[Event]) {}

    /// Runs the post-hook logic for this validator.
    ///
    /// This hook allows validator implementations to execute any operations, collect any data, and
    /// so on, after the component has finished its validation run and all outputs have been collected.
    ///
    /// The given `outputs` are all of the events received from the component being validated.
    fn run_post_hook(&mut self, _outputs: &[Event]) {}

    /// Consumes this validator, collecting and returning its results.
    fn into_results(self: Box<Self>) -> Result<Vec<String>, Vec<String>>;
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
            StandardValidators::ComponentSpec => Box::new(ComponentSpecValidator::default()),
        }
    }
}
