mod resources;
mod runner;
mod sync;
mod test_case;
pub mod util;
mod validators;

use crate::{sinks::Sinks, sources::Sources, transforms::Transforms};

pub use self::resources::*;
pub use self::runner::*;
pub use self::test_case::{TestCase, TestCaseExpectation};
pub use self::validators::*;

/// Component types that can be validated.
// TODO: We should centralize this in `vector-common` or something, where both this code and the
// configuration schema stuff (namely the proc macros that use this) can share it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
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
            let component_name = validatable_component.component_name();
            let component_type = validatable_component.component_type();
            info!(
                "Running validation for component '{}' (type: {:?})...",
                component_name, component_type
            );

            let mut runner = Runner::from_component(validatable_component);
            runner.add_validator(StandardValidators::ComponentSpec);

            match runner.run_validation().await {
                Ok(test_case_results) => {
                    let mut details = Vec::new();
                    let mut had_failures = false;

                    for (idx, test_case_result) in test_case_results.into_iter().enumerate() {
                        for validator_result in test_case_result.validator_results() {
                            match validator_result {
                                Ok(success) => {
                                    if success.is_empty() {
                                        details.push(format!("  test case #{}: passed", idx));
                                    } else {
                                        let formatted = success
                                            .iter()
                                            .map(|s| format!("    - {}\n", s))
                                            .collect::<Vec<_>>();

                                        details.push(format!(
                                            "  test case #{}: passed\n{}",
                                            idx,
                                            formatted.join("")
                                        ));
                                    }
                                }
                                Err(failure) => {
                                    had_failures = true;

                                    if failure.is_empty() {
                                        details.push(format!("  test case #{}: failed", idx));
                                    } else {
                                        let formatted = failure
                                            .iter()
                                            .map(|s| format!("    - {}\n", s))
                                            .collect::<Vec<_>>();

                                        details.push(format!(
                                            "  test case #{}: failed\n{}",
                                            idx,
                                            formatted.join("")
                                        ));
                                    }
                                }
                            }
                        }
                    }

                    if had_failures {
                        panic!(
                            "Failed to validate component '{}':\n{}",
                            component_name,
                            details.join("")
                        );
                    } else {
                        info!(
                            "Successfully validated component '{}':\n{}",
                            component_name,
                            details.join("")
                        );
                    }
                }
                Err(e) => panic!(
                    "Failed to complete validation run for component '{}': {}",
                    component_name, e
                ),
            }
        }
    }
}
