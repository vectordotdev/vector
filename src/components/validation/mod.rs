mod resources;
mod runner;
mod sync;
mod test_case;
mod validators;

use crate::{sinks::Sinks, sources::Sources, transforms::Transforms};

pub use self::resources::*;
pub use self::runner::*;
use self::sync::*;
pub use self::test_case::{TestCase, TestCaseExpectation};
pub use self::validators::*;

/// Component types that can be validated.
#[derive(Clone, Copy)]
pub enum ComponentType {
    Source,
    Transform,
    Sink,
}

/// Component type-specific configuration.
pub enum ComponentConfiguration {
    /// A source component.
    Source(Sources),

    /// A transform component.
    Transform(Transforms),

    /// A sink component.
    Sink(Sinks),
}

pub trait ValidatableComponent: Send + Sync {
    /// Gets the name of the component.
    fn component_name(&self) -> &'static str;

    /// Gets the type of the component.
    fn component_type(&self) -> ComponentType;

    /// Gets the component configuration.
    ///
    /// As building a component topology requires strongly-typed values for each component type,
    /// this method is expected to return a component-type variant of `ComponentConfiguration` that
    /// can be used to pass the component's configuration to `ConfigBuilder`.
    ///
    /// For example, a source is added to `ConfigBuilder` by providing a value that can be converted
    /// to `Sources`, the "big enum" that has a variant for every configurable source. For a source
    /// implementing this trait, it would return `ComponentConfiguration::Source(source)`, where
    /// `source` was a valid of `Sources` that maps to the given component's true configuration
    /// type.
    fn component_configuration(&self) -> ComponentConfiguration;

    /// Gets the external resource associated with this component.
    ///
    /// For sources and sinks, there is always an "external" resource, whether it's an address to
    /// listen on for traffic, or a Kafka cluster to send events to, and so on. `ExternalResource`
    /// defines what that external resource is in a semi-structured way, including the
    /// directionality i.e. pull vs push.
    ///
    /// Components inherently have their external resource either as an input (source) or an output
    /// (sink). For transforms, they are attached to components on both sides, so they require no
    /// external resource.
    // TODO: Should this be a vector for multiple resources? Does anything actually have multiple
    // external resource dependencies? Not necessarily in the sense of, say, the `file` source
    // monitoring multiple files, but a component that both listens on a TCP socket _and_ opens a
    // specific file, etc.
    fn external_resource(&self) -> Option<ExternalResource>;

    /// Gets the test cases to use for validating this component.
    ///
    /// Validation of a component can occur across multiple axes, such as validating the "happy"
    /// path, where we might only expect to see metrics/events related to successfully processing
    /// events, vs a failure path, where we would expect to see metrics/events related to failing to
    /// process events.
    ///
    /// Each validation test case describes both the expected outcome (success, failure, etc) as
    /// well as the events to send for each of those test cases. This allows components to ensure
    /// the right data is sent to properly trigger certain code paths.
    fn test_cases(&self) -> Vec<TestCase>;
}

impl<'a, T> ValidatableComponent for &'a T
where
    T: ValidatableComponent + ?Sized,
{
    fn component_name(&self) -> &'static str {
        (*self).component_name()
    }

    fn component_type(&self) -> ComponentType {
        (*self).component_type()
    }

    fn component_configuration(&self) -> ComponentConfiguration {
        (*self).component_configuration()
    }

    fn external_resource(&self) -> Option<ExternalResource> {
        (*self).external_resource()
    }

    fn test_cases(&self) -> Vec<TestCase> {
        (*self).test_cases()
    }
}

#[cfg(feature = "sources-http_server")]
#[cfg(test)]
mod tests {
    use crate::{
        components::validation::{Runner, StandardValidators},
        sources::http_server::SimpleHttpConfig,
    };

    use super::ValidatableComponent;

    fn get_all_validatable_components() -> Vec<&'static dyn ValidatableComponent> {
        // This method is the theoretical spot where we would collect all components that should be
        // validated by tapping into the component registration that we do with
        // `#[configurable_component]`, and so on.
        //
        // However, as that would require every component we get back from those mechanisms to implement
        // `Component`, we can't (yet) use them, so here's we're approximating that logic by creating
        // our own static version of a single component -- the `http_server` source -- and handing it
        // back.
        //
        // Yes, we're leaking an object. It's a test, who cares.
        vec![Box::leak(Box::new(SimpleHttpConfig::default()))]
    }

    #[tokio::test]
    async fn compliance() {
        crate::test_util::trace_init();

        let validatable_components = get_all_validatable_components();
        for validatable_component in validatable_components {
            let mut runner = Runner::from_component(validatable_component);
            runner.add_validator(StandardValidators::ComponentSpec);

            match runner.run_validation().await {
                Ok(test_case_results) => {
                    for test_case_result in test_case_results {
                        for validator_result in test_case_result.validator_results() {
                            match validator_result {
                                // Getting results in the success case will be rare, but perhaps we want to always print
                                // successful validations so that we can verify that specific components are being validated,
                                // and verify what things we're validating them against.
                                Ok(_success_results) => {}
                                Err(failure_results) => {
                                    let formatted_failures = failure_results
                                        .iter()
                                        .map(|s| format!(" - {}\n", s))
                                        .collect::<Vec<_>>();
                                    panic!(
                                        "Failed to validate component '{}':\n\n{}",
                                        validatable_component.component_name(),
                                        formatted_failures.join("")
                                    );
                                }
                            }
                        }
                    }
                }
                Err(e) => panic!(
                    "Failed to complete validation run for component '{}': {}",
                    validatable_component.component_name(),
                    e
                ),
            }
        }
    }
}
