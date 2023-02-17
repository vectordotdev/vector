use std::fmt;

use vector_config_common::{attributes::CustomAttribute, validation};

use crate::ToValue;

/// The metadata associated with a given type or field.
pub struct Metadata<T> {
    title: Option<&'static str>,
    description: Option<&'static str>,
    default_value: Option<Box<dyn ToValue>>,
    custom_attributes: Vec<CustomAttribute>,
    deprecated: bool,
    deprecated_message: Option<&'static str>,
    transparent: bool,
    validations: Vec<validation::Validation>,
    _dummy: std::marker::PhantomData<T>,
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

    pub fn default_value(&self) -> Option<&dyn ToValue> {
        self.default_value.as_deref()
    }

    pub fn set_default_value(&mut self, default_value: impl ToValue + 'static) {
        self.default_value = Some(Box::new(default_value));
    }

    pub fn deprecated(&self) -> bool {
        self.deprecated
    }

    pub fn set_deprecated(&mut self) {
        self.deprecated = true;
    }

    pub fn deprecated_message(&self) -> Option<&'static str> {
        self.deprecated_message
    }

    pub fn set_deprecated_message(&mut self, message: &'static str) {
        self.deprecated_message = Some(message);
    }

    pub fn with_transparent(transparent: bool) -> Self {
        Self {
            transparent,
            ..Default::default()
        }
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
            deprecated_message: other.deprecated_message.or(self.deprecated_message),
            transparent: other.transparent,
            validations: self.validations,
            _dummy: Default::default(),
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
            deprecated_message: self.deprecated_message,
            transparent: self.transparent,
            validations: self.validations.clone(),
            _dummy: Default::default(),
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
            deprecated_message: None,
            transparent: false,
            validations: Vec::new(),
            _dummy: Default::default(),
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
            .field("deprecated_message", &self.deprecated_message)
            .field("transparent", &self.transparent)
            .field("validations", &self.validations)
            .finish()
    }
}
