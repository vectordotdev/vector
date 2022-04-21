// High-level list of TODOS.
//
// TODO: `serde` supports defining a default at the struct level to fill in fields when no value is present during
// serialization, but it also supports defaults on a per-field basis, which override any defaults that would be applied
// by virtue of the struct-level default.
//
// Thus, we should mark a struct optional if it has a struct-level default _or_ if all fields are optional: either
// literal `Option<T>` fields or if they all have defaults.
//
// This could clean up some of the required properties where we have a field-level/struct-level default that we can
// check by looking at the metadata for the type implementing `T`, maybe even such that the default impl of
// `Configurable::is_optional` could just use that.
//
// TODO: What happens if we try to stick in a field that has a struct with a lifetime attached to it? How does the name
// of that get generated in terms of what ends up in the schema? Do we even have fields with lifetime bounds in any of
// our configuration types in `vector`? :thinking:
//
// TODO: We don't support `#[serde(flatten)]` either for collecting unknown fields or for flattening a field into its
// parent struct. However, per #12341, we might actually not want to allow using `flatten` for collecting unknown
// fields, at least, which would make implementing flatten support for merging structs a bit easier.

// TODO: Remove this once we implement validation support on the derive macro side, because most things that are dead
// are related to that.
#![allow(dead_code)]

use core::fmt;
use std::marker::PhantomData;

use schemars::{
    gen::SchemaGenerator,
    schema::{NumberValidation, SchemaObject, StringValidation},
};
use serde::{Deserialize, Serialize};
use vector_config_macros::Configurable;

pub mod schema;

mod stdlib;

pub use vector_config_macros::configurable_component;

const NUM_MANTISSA_BITS: u32 = 53;
const NUM_MAX_BOUND_UNSIGNED: u64 = 2u64.pow(NUM_MANTISSA_BITS);
const NUM_MIN_BOUND_SIGNED: i64 = -(2i64.pow(NUM_MANTISSA_BITS));
const NUM_MAX_BOUND_SIGNED: i64 = 2i64.pow(NUM_MANTISSA_BITS);

#[derive(Clone, Default)]
pub struct StringShape {
    minimum_length: Option<u32>,
    maximum_length: Option<u32>,
    allowed_pattern: Option<&'static str>,
}

impl From<StringShape> for StringValidation {
    fn from(s: StringShape) -> Self {
        StringValidation {
            max_length: s.maximum_length,
            min_length: s.minimum_length,
            pattern: s.allowed_pattern.map(|s| s.to_string()),
        }
    }
}

#[derive(Clone)]
pub enum NumberShape {
    Unsigned { minimum: u64, maximum: u64 },
    Signed { minimum: i64, maximum: i64 },
    FloatingPoint { minimum: f64, maximum: f64 },
}

impl NumberShape {
    pub fn unsigned(upper: u64) -> Self {
        NumberShape::Unsigned {
            minimum: 0,
            maximum: NUM_MAX_BOUND_UNSIGNED.min(upper),
        }
    }

    pub fn signed(lower: i64, upper: i64) -> Self {
        NumberShape::Signed {
            minimum: NUM_MIN_BOUND_SIGNED.min(lower),
            maximum: NUM_MAX_BOUND_SIGNED.min(upper),
        }
    }
}

impl From<NumberShape> for NumberValidation {
    fn from(s: NumberShape) -> Self {
        // SAFETY: Generally speaking, we don't like primitive casts -- `foo as ...` -- because they
        // can end up being silently lossy. That is certainly true here in the case of trying to
        // convert an i64 or u64 to f64.
        //
        // The reason it's (potentially) lossy is due to the internal layout of f64, where,
        // essentially, the mantissa is 53 bits, so it can precisely represent an integer up to 2^53
        // such that if you tried to convert 2^53 + 1 to an f64, and then back to an u64, you would
        // end up with a different value than 2^53 + 1.
        //
        // All of this is a long way of saying: we limit integers to 2^53 so that we can always be
        // sure that when we end up specifying their minimum/maximum in the schema, the values we
        // give can be represented concretely and losslessly. In turn, this makes the primitive
        // casts "safe", because we know we're not losing precision.
        let (minimum, maximum) = match s {
            NumberShape::Unsigned { minimum, maximum } => {
                if maximum > NUM_MAX_BOUND_UNSIGNED {
                    panic!(
                        "unsigned integers cannot have a maximum bound greater than 2^{}",
                        NUM_MANTISSA_BITS
                    );
                }

                (minimum as f64, maximum as f64)
            }
            NumberShape::Signed { minimum, maximum } => {
                if minimum > NUM_MIN_BOUND_SIGNED {
                    panic!(
                        "signed integers cannot have a minimum bound less than than -2^{}",
                        NUM_MANTISSA_BITS
                    );
                }

                if maximum > NUM_MAX_BOUND_SIGNED {
                    panic!(
                        "signed integers cannot have a maximum bound greater than 2^{}",
                        NUM_MANTISSA_BITS
                    );
                }

                (minimum as f64, maximum as f64)
            }
            NumberShape::FloatingPoint { minimum, maximum } => (minimum, maximum),
        };

        NumberValidation {
            minimum: Some(minimum),
            maximum: Some(maximum),
            ..Default::default()
        }
    }
}

#[derive(Clone)]
pub struct ArrayShape {
    minimum_length: Option<u32>,
    maximum_length: Option<u32>,
}

#[derive(Clone)]
pub struct Metadata<'de, T: Configurable<'de>> {
    title: Option<&'static str>,
    description: Option<&'static str>,
    default_value: Option<T>,
    custom_attributes: Vec<(&'static str, &'static str)>,
    deprecated: bool,
    transparent: bool,
    _de: PhantomData<&'de ()>,
}

impl<'de, T: Configurable<'de>> Metadata<'de, T> {
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

    pub fn with_default_value(default: T) -> Self {
        Self {
            default_value: Some(default),
            ..Default::default()
        }
    }

    pub fn default_value(&self) -> Option<T> {
        self.default_value.clone()
    }

    pub fn set_default_value(&mut self, default_value: T) {
        self.default_value = Some(default_value);
    }

    pub fn clear_default_value(&mut self) {
        self.default_value = None;
    }

    pub fn map_default_value<F, U>(self, f: F) -> Metadata<'de, U>
    where
        F: FnOnce(T) -> U,
        U: Configurable<'de>,
    {
        Metadata {
            title: self.title,
            description: self.description,
            default_value: self.default_value.map(f),
            custom_attributes: self.custom_attributes,
            deprecated: self.deprecated,
            transparent: self.transparent,
            _de: PhantomData,
        }
    }

    pub fn deprecated(&self) -> bool {
        self.deprecated
    }

    pub fn set_deprecated(&mut self) {
        self.deprecated = true;
    }

    pub fn clear_deprecated(&mut self) {
        self.deprecated = false;
    }

    pub fn transparent(&self) -> bool {
        self.transparent
    }

    pub fn set_transparent(&mut self) {
        self.transparent = true;
    }

    pub fn clear_transparent(&mut self) {
        self.transparent = false;
    }

    pub fn custom_attributes(&self) -> &[(&'static str, &'static str)] {
        &self.custom_attributes
    }

    pub fn add_custom_attribute(&mut self, key: &'static str, value: &'static str) {
        self.custom_attributes.push((key, value));
    }

    pub fn clear_custom_attribute(&mut self) {
        self.custom_attributes.clear();
    }

    pub fn merge(mut self, other: Metadata<'de, T>) -> Self {
        self.custom_attributes.extend(other.custom_attributes);

        Self {
            title: other.title.or(self.title),
            description: other.description.or(self.description),
            default_value: other.default_value.or(self.default_value),
            custom_attributes: self.custom_attributes,
            deprecated: other.deprecated,
            transparent: other.transparent,
            _de: PhantomData,
        }
    }

    pub fn convert<U: Configurable<'de>>(self) -> Metadata<'de, U> {
        Metadata {
            title: self.title,
            description: self.description,
            default_value: None,
            custom_attributes: self.custom_attributes,
            deprecated: self.deprecated,
            transparent: self.transparent,
            _de: PhantomData,
        }
    }
}

impl<'de, T: Configurable<'de>> Metadata<'de, Option<T>> {
    pub fn flatten_default(self) -> Metadata<'de, T> {
        Metadata {
            title: self.title,
            description: self.description,
            default_value: self.default_value.flatten(),
            custom_attributes: self.custom_attributes,
            deprecated: self.deprecated,
            transparent: self.transparent,
            _de: PhantomData,
        }
    }
}

impl<'de, T: Configurable<'de>> Default for Metadata<'de, T> {
    fn default() -> Self {
        Self {
            title: None,
            description: None,
            default_value: None,
            custom_attributes: Vec::new(),
            deprecated: false,
            transparent: false,
            _de: PhantomData,
        }
    }
}

impl<'de, T: Configurable<'de>> fmt::Debug for Metadata<'de, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Metadata")
            .field("title", &self.title)
            .field("description", &self.description)
            .field(
                "default",
                if self.default_value.is_some() {
                    &"<some>"
                } else {
                    &"<none>"
                },
            )
            .field("attributes", &self.custom_attributes)
            .finish()
    }
}

pub trait Configurable<'de>: Serialize + Deserialize<'de> + Sized
where
    Self: Clone,
{
    /// Gets the referencable name of this value, if any.
    ///
    /// When specified, this implies the value is both complex and standardized, and should be
    /// reused within any generated schema it is present in.
    fn referencable_name() -> Option<&'static str> {
        None
    }

    /// Gets the human-readable description of this value, if any.
    ///
    /// For standard types, this will be `None`. Commonly, custom types would implement this
    /// directly, while fields using standard types would provide a field-specific description that
    /// would be used instead of the default descrption.
    fn description() -> Option<&'static str> {
        None
    }

    /// Whether or not this value is optional.
    fn is_optional() -> bool {
        false
    }

    /// Gets the metadata for this value.
    fn metadata() -> Metadata<'de, Self> {
        let mut metadata = Metadata::default();
        if let Some(description) = Self::description() {
            metadata.set_description(description);
        }
        metadata
    }

    /// Generates the schema for this value.
    fn generate_schema(gen: &mut SchemaGenerator, overrides: Metadata<'de, Self>) -> SchemaObject;
}
