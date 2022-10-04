use std::marker::PhantomData;

use snafu::Snafu;
use toml::Value;

use super::{ComponentMarker, GenerateConfig};

#[derive(Debug, Snafu, Clone, PartialEq, Eq)]
pub enum ExampleError {
    #[snafu(display("unable to create an example for this component"))]
    MissingExample,

    #[snafu(display("component '{}' does not exist", component_name))]
    DoesNotExist { component_name: String },
}

/// Description of a component.
pub struct ComponentDescription<T: ComponentMarker + Sized> {
    component_name: &'static str,
    example_value: fn() -> Option<Value>,
    _component_type: PhantomData<T>,
}

impl<T> ComponentDescription<T>
where
    T: ComponentMarker + Sized + 'static,
    inventory::iter<ComponentDescription<T>>:
        std::iter::IntoIterator<Item = &'static ComponentDescription<T>>,
{
    /// Creates a new `ComponentDescription`.
    ///
    /// This creates a component description for a component identified both by the given component
    /// type `T` and the component name. As such, if `T` is `SourceComponent`, and the name is
    /// `stdin`, you would say that the component is a "source called `stdin`".
    ///
    /// The type parameter `C` must be the component's configuration type that implements `GenerateConfig`.
    pub const fn new<C: GenerateConfig>(component_name: &'static str) -> Self {
        ComponentDescription {
            component_name,
            example_value: || Some(C::generate_config()),
            _component_type: PhantomData,
        }
    }

    /// Generates an example configuration for the component with the given component name.
    ///
    /// ## Errors
    ///
    /// If no component, identified by `T` and the given name, is registered, or if there is an
    /// error generating the example configuration, an error variant will be returned.
    pub fn example(component_name: &str) -> Result<Value, ExampleError> {
        inventory::iter::<ComponentDescription<T>>
            .into_iter()
            .find(|t| t.component_name == component_name)
            .ok_or_else(|| ExampleError::DoesNotExist {
                component_name: component_name.to_owned(),
            })
            .and_then(|t| (t.example_value)().ok_or(ExampleError::MissingExample))
    }

    /// Gets a sorted list of all registered components of the given component type.
    pub fn types() -> Vec<&'static str> {
        let mut types = Vec::new();
        for definition in inventory::iter::<ComponentDescription<T>> {
            types.push(definition.component_name);
        }
        types.sort_unstable();
        types
    }
}
