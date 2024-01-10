use std::borrow::Cow;

use serde_json::Value;
use snafu::Snafu;
use vector_config_common::{
    attributes::CustomAttribute,
    constants::{self, ComponentType},
    schema::SchemaObject,
};

use super::query::{OneOrMany, QueryError, QueryableSchema, SchemaType, SimpleSchema};

#[derive(Debug, Snafu)]
pub enum SchemaError {
    #[snafu(display("invalid component schema: {pointer}: {reason}"))]
    InvalidComponentSchema {
        pointer: &'static str,
        reason: Cow<'static, str>,
    },
}

impl SchemaError {
    pub fn invalid_component_schema<S: Into<Cow<'static, str>>>(
        pointer: &'static str,
        reason: S,
    ) -> Self {
        Self::InvalidComponentSchema {
            pointer,
            reason: reason.into(),
        }
    }
}

/// A schema object that represents the schema of a single Vector component.
///
/// The schema represents the equivalent of the component's configuration type, excluding any common
/// configuration fields that appear on a per-component type basis. This means that, for a sink
/// component, this schema would include the configuration fields of the specific sink component,
/// but wouldn't contain the common sink configuration fields such as `inputs` or `buffer`.
pub struct ComponentSchema<'a> {
    schema: &'a SchemaObject,
    component_name: String,
    component_type: ComponentType,
}

impl<'a> ComponentSchema<'a> {
    /// The type of the component represented by this schema.
    pub fn component_type(&self) -> ComponentType {
        self.component_type
    }

    /// The name of the component represented by this schema.
    ///
    /// This refers to the configuration-specific identifier used to specify the component type
    /// within the `type` field.
    ///
    /// For example, the AWS S3 sink would be `aws_s3`.
    pub fn component_name(&self) -> &str {
        &self.component_name
    }
}

impl<'a> QueryableSchema for ComponentSchema<'a> {
    fn schema_type(&self) -> SchemaType {
        self.schema.schema_type()
    }

    fn description(&self) -> Option<&str> {
        self.schema.description()
    }

    fn title(&self) -> Option<&str> {
        self.schema.title()
    }

    fn get_attributes(&self, key: &str) -> Option<OneOrMany<CustomAttribute>> {
        self.schema.get_attributes(key)
    }

    fn get_attribute(&self, key: &str) -> Result<Option<CustomAttribute>, QueryError> {
        self.schema.get_attribute(key)
    }

    fn has_flag_attribute(&self, key: &str) -> Result<bool, QueryError> {
        self.schema.has_flag_attribute(key)
    }
}

impl<'a> TryFrom<SimpleSchema<'a>> for ComponentSchema<'a> {
    type Error = SchemaError;

    fn try_from(value: SimpleSchema<'a>) -> Result<Self, Self::Error> {
        // Component schemas must have a component type _and_ component name defined.
        let component_type =
            get_component_metadata_kv_str(&value, constants::DOCS_META_COMPONENT_TYPE).and_then(
                |s| {
                    ComponentType::try_from(s.as_str()).map_err(|_| {
                        SchemaError::invalid_component_schema(
                            constants::DOCS_META_COMPONENT_TYPE,
                            "value was not a valid component type",
                        )
                    })
                },
            )?;

        let component_name =
            get_component_metadata_kv_str(&value, constants::DOCS_META_COMPONENT_NAME)?;

        Ok(Self {
            schema: value.into_inner(),
            component_name,
            component_type,
        })
    }
}

fn get_component_metadata_kv_str<'a>(
    schema: &'a SimpleSchema<'a>,
    key: &'static str,
) -> Result<String, SchemaError> {
    schema
        .get_attribute(key)
        .map_err(|e| SchemaError::invalid_component_schema(key, e.to_string()))?
        .ok_or_else(|| SchemaError::invalid_component_schema(key, "attribute must be present"))
        .and_then(|attr| match attr {
            CustomAttribute::Flag(_) => Err(SchemaError::invalid_component_schema(
                key,
                "expected key/value attribute, got flag instead",
            )),
            CustomAttribute::KeyValue { value, .. } => Ok(value),
        })
        .and_then(|v| match v {
            Value::String(name) => Ok(name),
            _ => Err(SchemaError::invalid_component_schema(
                key,
                format!("`{}` must be a string", key),
            )),
        })
}
