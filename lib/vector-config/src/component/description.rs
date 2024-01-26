use std::{cell::RefCell, marker::PhantomData};

use snafu::Snafu;
use toml::Value;
use vector_config_common::{attributes::CustomAttribute, constants};

use super::{ComponentMarker, GenerateConfig};
use crate::schema::{SchemaGenerator, SchemaObject};
use crate::{schema, Configurable, ConfigurableRef, GenerateError, Metadata};

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
    description: &'static str,
    label: &'static str,
    logical_name: &'static str,
    example_value: fn() -> Option<Value>,
    config: ConfigurableRef,
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
    /// The type parameter `C` must be the component's configuration type that implements
    /// `Configurable` and `GenerateConfig`.
    pub const fn new<C: GenerateConfig + Configurable + 'static>(
        component_name: &'static str,
        label: &'static str,
        logical_name: &'static str,
        description: &'static str,
    ) -> Self {
        ComponentDescription {
            component_name,
            description,
            label,
            logical_name,
            example_value: || Some(C::generate_config()),
            config: ConfigurableRef::new::<C>(),
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

    /// Generate a schema object covering all the descriptions of this type.
    pub fn generate_schemas(gen: &RefCell<SchemaGenerator>) -> Result<SchemaObject, GenerateError> {
        let mut descriptions: Vec<_> = inventory::iter::<Self>.into_iter().collect();
        descriptions.sort_unstable_by_key(|desc| desc.component_name);
        let subschemas: Vec<SchemaObject> = descriptions
            .into_iter()
            .map(|description| description.generate_schema(gen))
            .collect::<Result<_, _>>()?;
        Ok(schema::generate_one_of_schema(&subschemas))
    }

    /// Generate a schema object for this description.
    fn generate_schema(
        &self,
        gen: &RefCell<SchemaGenerator>,
    ) -> Result<SchemaObject, GenerateError> {
        let mut tag_subschema =
            schema::generate_const_string_schema(self.component_name.to_string());
        let variant_tag_metadata = Metadata::with_description(self.description);
        schema::apply_base_metadata(&mut tag_subschema, variant_tag_metadata);

        let tag_schema =
            schema::generate_internal_tagged_variant_schema("type".to_string(), tag_subschema);
        let flattened_subschemas = vec![tag_schema];

        let mut field_metadata = Metadata::default();
        field_metadata.set_transparent();
        let mut subschema =
            schema::get_or_generate_schema(&self.config, gen, Some(field_metadata))?;

        schema::convert_to_flattened_schema(&mut subschema, flattened_subschemas);

        let mut variant_metadata = Metadata::default();
        variant_metadata.set_description(self.description);
        variant_metadata.add_custom_attribute(CustomAttribute::kv(
            constants::DOCS_META_HUMAN_NAME,
            self.label,
        ));
        variant_metadata
            .add_custom_attribute(CustomAttribute::kv("logical_name", self.logical_name));
        schema::apply_base_metadata(&mut subschema, variant_metadata);

        Ok(subschema)
    }
}
