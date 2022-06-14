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
// TODO: Is there a way that we could attempt to brute force detect the types of fields being used with a validation to
// give a compile-time error when validators are used incorrectly? For example, we throw a runtime error if you use a
// negative `min` range bound on an unsigned integer field, but it's a bit opaque and hard to decipher.  Could we simply
// brute force match the qualified path field to see if there's any known unsigned integer type in it -- i.e. `u8`,
// `u64`, etc -- and then throw a compile-error from the macro? We would still end up throwing an error at runtime if
// our heuristic to detect unsigned integers failed, but we might be able to give a meaningful error closer to the
// problem, which would be much better.
//
// TODO: If we want to deny unknown fields on structs, JSON Schema supports that by setting `additionalProperties` to
// `false` on a schema, which turns it into a "closed" schema. However, this is at odds with types used in enums, which
// is all of our component configuration types. This is because applying `additionalProperties` to the configuration
// type's schema itself would consider something like an internal enum tag (i.e. `"type": "aws_s3"`) as an additional
// property, even if `type` was already accounted for in another subschema that was validated against.
//
// JSON Schema draft 2019-09 has a solution for this -- `unevaluatedProperties` -- which forces the validator to track
// what properties have been "accounted" for, so far, during subschema validation during things like validating against
// all subschemas in `allOf`.
//
// Essentially, we should force all structs to generate a schema that sets `additionalProperties` to `false`, but if it
// gets used in a way that will place it into `allOf` (which is the case for internally tagged enum variants aka all
// component configuration types) then we need to update the schema codegen to unset that field, and re-apply it as
// `unevaluatedProperties` on the schema which is using `allOf`.
//
// Logically, this makes sense because we're only creating a new wrapper schema B around some schema A such that we can
// use it as a tagged enum variant, so rules like "no additional properties" should apply to the wrapper, since schema A
// and B should effectively represent the same exact thing.
//
// TODO: We may want to simply switch from using `description` as the baseline descriptive field to using `title`.
// While, by itself, I think `description` makes a little more sense than `title`, it makes it hard to do split-location
// documentation.
//
// For example, it would be nice to have helper types (i.e. `BatchConfig`, `MultilineConfig`, etc) define their own
// titles, and then allow other structs that have theor types as fields specify a description. This would be very useful
// in cases where fields are optional, such that you want the field's title to be the title of the underlying type (e.g.
// "Multi-line parsing configuration.") but you want the field's description to say something like "If not specified,
// then multiline parsing is disabled". Including that description on `MultilineConfig` itself is kind of weird because
// it forces that on everyone else using it, where, in some cases, it may not be optional at all.
//
// TODO: Right now, we're manually generating a referencable name where it makes sense by appending the module path to
// the ident for structs/enums, and by crafting the name by hand for anything like stdlib impls, or impls on external
// types.
//
// We do this because technically `std::any::type_name` says that it doesn't provide a stable interface for getting the
// fully-qualified path of a type, which we would need (in general, regardless of whether or not we used that function)
// because we don't want definition types totally changing name between compiler versions, etc.
//
// This is obviously also tricky from a re-export standpoint i.e. what is the referencable name of a type that uses the
// derive macros for `Configurable` but is exporter somewhere entirely different? The path would refer to the source nol
// matter what, as it's based on how `std::module_path!()` works. Technically speaking, that's still correct from a "we
// shouldn't create duplicate schemas for T" standpoint, but could manifest as a non-obvious divergence.
//
// TODO: We need to figure out how to handle aliases. Looking previously, it seemed like we might need to do some very
// ugly combinatorial explosion stuff to define a schema per perumtation of all aliased fields in a config. We might be
// able to get away with using a combination of `allOf` and `oneOf` where we define a subschema for the non-aliased
// fields, and then a subschema using `oneOf`for each aliased field -- allowing it to match any of the possible field
// names for that specific field -- and then combine them all with `allOf`, which keeps the schema as compact as
// possible, I think, short of a new version of the specification coming out that adds native alias support for
// properties.
//
// TODO: Add support for defining metadata on fields, since each field is defined as a schema unto itself, so we can
// stash metadata in the extensions for each field the same as we do for structs.
//
// TODO: Add support for single value metadata entries, in addition to key/value, such that for things like field metadata, we
// can essentially define flags i.e. `docs:templateable` as a metadata value for marking a field as working with
// Vector's template syntax, since doing `templateable = true` is weird given that we never otherwise specifically
// disable it. In other words, we want a way to define feature flags in metadata.

use core::fmt;
use core::marker::PhantomData;

use num::ConfigurableNumber;
use serde::{Deserialize, Serialize};

pub mod schema;

// Re-export of the various public dependencies required by the generated code to simplify the import requirements for
// crates actually using the macros/derives.
pub mod indexmap {
    pub use indexmap::*;
}
pub mod schemars {
    pub use schemars::*;
}

mod external;
mod num;
mod stdlib;

// Re-export of the `#[configurable_component]` and `#[derive(Configurable)]` proc macros.
pub use vector_config_macros::*;

// Re-export of both `Format` and `Validation` from `vector_config_common`.
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

/// A type that can be represented in a Vector configuration.
///
/// In Vector, we want to be able to generate a schema for our configuration such that we can have a Rust-agnostic
/// definition of exactly what is configurable, what values are allowed, what bounds exist, and so on and so forth.
///
/// `Configurable` provides the machinery to allow describing and encoding the shape of a type, recursively, so that by
/// instrumenting all transitive types of the configuration, the schema can be discovered by generating the schema from
/// some root type.
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
    fn generate_schema(
        gen: &mut schemars::gen::SchemaGenerator,
        overrides: Metadata<'de, Self>,
    ) -> schemars::schema::SchemaObject;
}

#[doc(hidden)]
pub fn __ensure_numeric_validation_bounds<'de, N>(metadata: &Metadata<'de, N>)
where
    N: Configurable<'de> + ConfigurableNumber,
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
    let mechanical_min_bound = N::get_enforced_min_bound();
    let mechanical_max_bound = N::get_enforced_max_bound();

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
