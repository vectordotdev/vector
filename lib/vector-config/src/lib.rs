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
//
// TODO: Is there a way that we could attempt to brute force detect the types of fields being used with a validation to
// give a compile-time error when validators are used incorrectly? For example, we throw a runtime error if you use a
// negative `min` range bound on an unsigned integer field, but it's a bit opaque and hard to decipher.  Could we simply
// brute force match the qualified path field to see if there's any known unsigned integer type in it -- i.e. `u8`,
// `u64`, etc -- and then throw a compile-error from the macro? We would still end up throwing an error at runtime if
// our heuristic to detect unsigned integers failed, but we might be able to give a meaningful error closer to the
// problem, which would be much better.

use core::fmt;
use core::marker::PhantomData;

use num_traits::{Bounded, ToPrimitive};
use schemars::{gen::SchemaGenerator, schema::SchemaObject};
use serde::{Deserialize, Serialize};

pub mod schema;

mod stdlib;

// Re-export of the `#[configurable_component]` and `#[derive(Configurable)]` proc macros.
pub use vector_config_macros::*;

// Re-export of both `Format` and `Validation` from `vetor_config_common`.
//
// The crate exists so that both `vector_config_macros` and `vector_config` can import the types and work with them
// natively, but from a codegen and usage perspective, it's much cleaner to export everything needed to use
// `Configurable` from `vector_config` itself, and not leak out the crate arrangement as an impl detail.
pub mod validation {
    pub use vector_config_common::validation::*;
}
#[derive(Clone)]
pub struct Metadata<'de, T: Configurable<'de>> {
    title: Option<&'static str>,
    description: Option<&'static str>,
    default_value: Option<T>,
    custom_attributes: Vec<(&'static str, &'static str)>,
    deprecated: bool,
    transparent: bool,
    validations: Vec<validation::Validation>,
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
            validations: self.validations,
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

    pub fn clear_custom_attributes(&mut self) {
        self.custom_attributes.clear();
    }

    pub fn validations(&self) -> &[validation::Validation] {
        &self.validations
    }

    pub fn add_validation(&mut self, validation: validation::Validation) {
        self.validations.push(validation);
    }

    pub fn clear_validations(&mut self) {
        self.validations.clear();
    }

    pub fn merge(mut self, other: Metadata<'de, T>) -> Self {
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
            validations: self.validations,
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
            validations: self.validations,
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
            validations: Vec::new(),
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

#[doc(hidden)]
pub fn __ensure_numeric_validation_bounds<'de, N>(metadata: &Metadata<'de, N>)
where
    N: Configurable<'de> + Bounded + ToPrimitive,
{
    // In `Validation::ensure_conformance`, we do some checks on any supplied numeric bounds to try and ensure they're
    // no larger than the largest f64 value where integer/floasting-point conversions are still lossless.  What we
    // cannot do there, however, is ensure that the bounds make sense for the type on the Rust side, such as a user
    // supplying a negative bound which would be fine for `i64`/`f64` but not for `u64`. That's where this function
    // comes in.
    //
    // We simply check the given metadata for any numeric validation bounds, and ensure they do not exceed the
    // mechanical limits of the given numeric type `N`.  If they do, we panic, which is not as friendly as a contextual
    // compile-time error emitted from the `Configurable` derive macro... but we're working with what we've got here.
    let mechanical_min_bound = N::min_value()
        .to_f64()
        .expect("`Configurable` does not support numbers larger than an f64 representation");
    let mechanical_max_bound = N::max_value()
        .to_f64()
        .expect("`Configurable` does not support numbers larger than an f64 representation");

    for validation in metadata.validations() {
        if let validation::Validation::Range { minimum, maximum } = validation {
            if let Some(min_bound) = minimum {
                if *min_bound < mechanical_min_bound {
                    panic!("invalid minimum in range validation for {}: has mechanical lower bound of {}, but {} was given", std::any::type_name::<N>(), mechanical_min_bound, min_bound);
                }
            }

            if let Some(max_bound) = maximum {
                if *max_bound > mechanical_max_bound {
                    panic!("invalid maximum in range validation for {}: has mechanical upper bound of {}, but {} was given", std::any::type_name::<N>(), mechanical_max_bound, max_bound);
                }
            }
        }
    }
}
