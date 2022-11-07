mod resources;
mod runner;
mod sync;
mod validation;

use async_trait::async_trait;
use vector_core::{sink::VectorSink, source::Source, transform::Transform};

use crate::config::{SinkContext, SourceContext, TransformContext};

pub use self::resources::*;
pub use self::runner::*;
use self::sync::*;
pub use self::validation::*;

/// Component types that can be validated.
pub enum ComponentType {
    Source,
    Transform,
    Sink,
}

/// Component-specific parts required to building the component.
pub enum ComponentBuilderParts {
    Source(SourceContext),
    Transform(TransformContext),
    Sink(SinkContext),
}

impl ComponentBuilderParts {
    pub fn into_source_builder_parts(self) -> SourceContext {
        match self {
            Self::Source(ctx) => ctx,
            _ => panic!("component builder parts are not for source"),
        }
    }

    pub fn into_transform_builder_parts(self) -> TransformContext {
        match self {
            Self::Transform(ctx) => ctx,
            _ => panic!("component builder parts are not for transform"),
        }
    }

    pub fn into_sink_builder_parts(self) -> SinkContext {
        match self {
            Self::Sink(ctx) => ctx,
            _ => panic!("component builder parts are not for sink"),
        }
    }
}

/// A built component.
pub enum BuiltComponent {
    Source(Source),
    Transform(Transform),
    Sink(VectorSink),
}

impl BuiltComponent {
    fn into_source_component(self) -> Source {
        match self {
            Self::Source(source) => source,
            _ => panic!("source component returned built component of different type"),
        }
    }

    fn into_transform_component(self) -> Transform {
        match self {
            Self::Transform(transform) => transform,
            _ => panic!("transform component returned built component of different type"),
        }
    }

    fn into_sink_component(self) -> VectorSink {
        match self {
            Self::Sink(sink) => sink,
            _ => panic!("sink component returned built component of different type"),
        }
    }
}

#[async_trait]
pub trait ValidatableComponent: Send + Sync {
    /// Gets the name of the component.
    fn component_name(&self) -> &'static str;

    /// Gets the type of the component.
    fn component_type(&self) -> ComponentType;

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

    /// Builds the runnable portion of a component.
    ///
    /// Given that this trait covers multiple component types, `ComponentBuilderParts` provides an
    /// opaque set of component type-specific parts needed for building a component. If the builder
    /// parts do not match the actual component type, `Err(...)` is returned with an error
    /// describing this. Alternatively, if the builder parts are correct but there is a general
    /// error with building the component, `Err(...)` is also returned.
    ///
    /// Otherwise, `Ok(...)` is returned, containing the built component.
    async fn build_component(
        &self,
        builder_parts: ComponentBuilderParts,
    ) -> Result<BuiltComponent, String>;
}

#[async_trait]
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

    fn external_resource(&self) -> Option<ExternalResource> {
        (*self).external_resource()
    }

    async fn build_component(
        &self,
        builder_parts: ComponentBuilderParts,
    ) -> Result<BuiltComponent, String> {
        (*self).build_component(builder_parts).await
    }
}

#[cfg(feature = "sources-http_server")]
#[cfg(test)]
mod tests {
    use crate::{
        components::compliance::{Runner, StandardValidators},
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

    #[test]
    fn compliance() {
        crate::test_util::trace_init();

        let validatable_components = get_all_validatable_components();
        for validatable_component in validatable_components {
            let mut runner = Runner::from_component(validatable_component);
            runner.add_validator(StandardValidators::ComponentSpec);

            match runner.run_validation() {
                Ok(results) => {
                    for validator_result in results.validator_results() {
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
                Err(e) => panic!(
                    "Failed to complete validation run for component '{}': {}",
                    validatable_component.component_name(),
                    e
                ),
            }
        }
    }
}
