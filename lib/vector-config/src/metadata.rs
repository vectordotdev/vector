use std::fmt;

use vector_config_common::{attributes::CustomAttribute, validation};

use crate::Configurable;

/// The metadata associated with a given type or field.
#[derive(Clone)]
pub struct Metadata<T> {
    title: Option<&'static str>,
    description: Option<&'static str>,
    default_value: Option<T>,
    custom_attributes: Vec<CustomAttribute>,
    deprecated: bool,
    transparent: bool,
    validations: Vec<validation::Validation>,
}

impl<T> Metadata<T> {
    pub fn with_title(title: &'static str) -> Self {
        Self {
            title: Some(title),
            ..Default::default()
        }
    }

    pub fn title(&self) -> Option<&'static str> {
        self.title
    }

    pub fn set_title(&mut self, title: &'static str) {
        self.title = Some(title);
    }

    pub fn clear_title(&mut self) {
        self.title = None;
    }

    pub fn with_description(desc: &'static str) -> Self {
        Self {
            description: Some(desc),
            ..Default::default()
        }
    }

    pub fn description(&self) -> Option<&'static str> {
        self.description
    }

    pub fn set_description(&mut self, desc: &'static str) {
        self.description = Some(desc);
    }

    pub fn clear_description(&mut self) {
        self.description = None;
    }

    pub fn default_value(&self) -> Option<&T> {
        self.default_value.as_ref()
    }

    pub fn with_default_value(default: T) -> Self {
        Self {
            default_value: Some(default),
            ..Default::default()
        }
    }

    pub fn set_default_value(&mut self, default_value: T) {
        self.default_value = Some(default_value);
    }

    pub fn consume_default_value(&mut self) -> Option<T> {
        self.default_value.take()
    }

    pub fn map_default_value<F, U>(self, f: F) -> Metadata<U>
    where
        F: FnOnce(T) -> U,
        U: Configurable,
    {
        Metadata {
            title: self.title,
            description: self.description,
            default_value: self.default_value.map(f),
            custom_attributes: self.custom_attributes,
            deprecated: self.deprecated,
            transparent: self.transparent,
            validations: self.validations,
        }
    }

    pub fn deprecated(&self) -> bool {
        self.deprecated
    }

    pub fn set_deprecated(&mut self) {
        self.deprecated = true;
    }

    pub fn transparent(&self) -> bool {
        self.transparent
    }

    pub fn set_transparent(&mut self) {
        self.transparent = true;
    }

    pub fn custom_attributes(&self) -> &[CustomAttribute] {
        &self.custom_attributes
    }

    pub fn add_custom_attribute(&mut self, attribute: CustomAttribute) {
        self.custom_attributes.push(attribute);
    }

    pub fn validations(&self) -> &[validation::Validation] {
        &self.validations
    }

    pub fn add_validation(&mut self, validation: validation::Validation) {
        self.validations.push(validation);
    }

    pub fn merge(mut self, other: Metadata<T>) -> Self {
        self.custom_attributes.extend(other.custom_attributes);
        self.validations.extend(other.validations);

        Self {
            title: other.title.or(self.title),
            description: other.description.or(self.description),
            default_value: other.default_value.or(self.default_value),
            custom_attributes: self.custom_attributes,
            deprecated: other.deprecated,
            transparent: other.transparent,
            validations: self.validations,
        }
    }

    /// Converts this metadata from holding a default value of `T` to `U`.
    ///
    /// If a default value was present before, it is dropped.
    pub fn convert<U>(&self) -> Metadata<U> {
        Metadata {
            title: self.title,
            description: self.description,
            default_value: None,
            custom_attributes: self.custom_attributes.clone(),
            deprecated: self.deprecated,
            transparent: self.transparent,
            validations: self.validations.clone(),
        }
    }
}

impl<T> Default for Metadata<T> {
    fn default() -> Self {
        Self {
            title: None,
            description: None,
            default_value: None,
            custom_attributes: Vec::new(),
            deprecated: false,
            transparent: false,
            validations: Vec::new(),
        }
    }
}

impl<T> fmt::Debug for Metadata<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Metadata")
            .field("title", &self.title)
            .field("description", &self.description)
            .field(
                "default_value",
                if self.default_value.is_some() {
                    &"<some>"
                } else {
                    &"<none>"
                },
            )
            .field("custom_attributes", &self.custom_attributes)
            .field("deprecated", &self.deprecated)
            .field("transparent", &self.transparent)
            .field("validations", &self.validations)
            .finish()
    }
}
