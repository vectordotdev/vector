// High-level list of TODOS.
//
// TODO: `serde` supports defining a default at the struct level to fill in fields when no value is
// present during serialization, but it also supports defaults on a per-field basis, which override
// any defaults that would be applied by virtue of the struct-level default.
//
// Thus, we should mark a struct optional if it has a struct-level default _or_ if all fields are
// optional: either literal `Option<T>` fields or if they all have defaults.
//
// This could clean up some of the required properties where we have a field-level/struct-level
// default that we can check by looking at the metadata for the type implementing `T`.
//
// TODO: What happens if we try to stick in a field that has a struct with a lifetime attached to
// it? How does the name of that get generated in terms of what ends up in the schema? Do we even
// have fields with lifetime bounds in any of our configuration types in `vector`? :thinking:
//
// TODO: Is there a way that we could attempt to brute force detect the types of fields being used
// with a validation to give a compile-time error when validators are used incorrectly? For example,
// we throw a runtime error if you use a negative `min` range bound on an unsigned integer field,
// but it's a bit opaque and hard to decipher.  Could we simply brute force match the qualified path
// field to see if there's any known unsigned integer type in it -- i.e. `u8`, `u64`, etc -- and
// then throw a compile-error from the macro? We would still end up throwing an error at runtime if
// our heuristic to detect unsigned integers failed, but we might be able to give a meaningful error
// closer to the problem, which would be much better.
//
// TODO: We may want to simply switch from using `description` as the baseline descriptive field to
// using `title`.  While, by itself, I think `description` makes a little more sense than `title`,
// it makes it hard to do split-location documentation.
//
// For example, it would be nice to have helper types (i.e. `BatchConfig`, `MultilineConfig`, etc)
// define their own titles, and then allow other structs that have theor types as fields specify a
// description. This would be very useful in cases where fields are optional, such that you want the
// field's title to be the title of the underlying type (e.g.  "Multi-line parsing configuration.")
// but you want the field's description to say something like "If not specified, then multiline
// parsing is disabled". Including that description on `MultilineConfig` itself is kind of weird
// because it forces that on everyone else using it, where, in some cases, it may not be optional at
// all.
//
// TODO: Right now, we're manually generating a referenceable name where it makes sense by appending
// the module path to the ident for structs/enums, and by crafting the name by hand for anything
// like stdlib impls, or impls on external types.
//
// We do this because technically `std::any::type_name` says that it doesn't provide a stable
// interface for getting the fully-qualified path of a type, which we would need (in general,
// regardless of whether or not we used that function) because we don't want definition types
// totally changing name between compiler versions, etc.
//
// This is obviously also tricky from a re-export standpoint i.e. what is the referenceable name of
// a type that uses the derive macros for `Configurable` but is exported somewhere entirely
// different? The path would refer to the source no matter what, as it's based on how
// `std::module_path!()` works. Technically speaking, that's still correct from a "we shouldn't
// create duplicate schemas for T" standpoint, but could manifest as a non-obvious divergence.
//
// TODO: We need to figure out how to handle aliases. Looking previously, it seemed like we might
// need to do some very ugly combinatorial explosion stuff to define a schema per permutation of all
// aliased fields in a config. We might be able to get away with using a combination of `allOf` and
// `oneOf` where we define a subschema for the non-aliased fields, and then a subschema using
// `oneOf`for each aliased field -- allowing it to match any of the possible field names for that
// specific field -- and then combine them all with `allOf`, which keeps the schema as compact as
// possible, I think, short of a new version of the specification coming out that adds native alias
// support for properties.
//
// TODO: Should we add a way, and/or make it the default, that if you only supply a description of a
// field, it concats the description of the type of the field? for example, you have:
//
// /// Predefined ACLs.
// ///
// /// For more information, see this link.
// pub enum PredefinedAcl { ...
// }
//
// and then somewhere else, you use it like this:
//
// struct Foo {
// ...
//     /// The Predefined ACL to apply to newly created objects.
//     field: PredefinedAcl,
// ...
// }
//
// the resulting docs for `field` should look as if we wrote this directly:
//
// /// The Predefined ACL to apply to newly created objects.
// ///
// /// For more information, see this link.
//
// Basically, we're always documenting these shared types fully, but sometimes their title is
// written in an intentionally generic way, and we may want to spice up the wording so it's
// context-specific i.e. we're using predefined ACLs for new objects, or using it for new firewall
// rules, or ... so on and so forth. and by concating the existing description on the shared type,
// we can continue to include high-quality doc comments with contextual links, examples, etc and
// avoid duplication.
//
// One question there would be: do we concat the description of the field _and_ the field's type
// together? We would probably have to, since the unwritten rule is to use link references, which
// are shoved at the end of the description like a footnote, and if we have a link reference in our
// field's title, then we need the field's description to be concatenated so that it can be resolved.
//
// TODO: Should we always apply the transparent marker to fields when they're the only field in a
// tuple struct/tuple variant? There's also some potential interplay with using the `derived` helper
// attribute on the tuple struct/tuple variant itself to signal that we want to pull the
// title/description from the field instead, which could be useful when using newtype wrappers
// around existing/remote types for the purpose of making them `Configurable`.
#![deny(warnings)]

// Re-export of the various public dependencies required by the generated code to simplify the import requirements for
// crates actually using the macros/derives.
pub mod indexmap {
    pub use indexmap::*;
}

pub use serde_json;

pub mod component;
mod configurable;
pub use self::configurable::{Configurable, ConfigurableRef, ToValue};
mod errors;
pub use self::errors::{BoundDirection, GenerateError};
mod external;
mod http;
mod metadata;
pub use self::metadata::Metadata;
mod named;
pub use self::named::NamedComponent;
mod num;
pub use self::num::ConfigurableNumber;
pub mod schema;
pub mod ser;
mod stdlib;
mod str;
pub use self::str::ConfigurableString;

// Re-export of the `#[configurable_component]` and `#[derive(Configurable)]` proc macros.
pub use vector_config_macros::*;

// Re-export of both `Format` and `Validation` from `vector_config_common`.
//
// The crate exists so that both `vector_config_macros` and `vector_config` can import the types and work with them
// natively, but from a codegen and usage perspective, it's much cleaner to export everything needed to use
// `Configurable` from `vector_config` itself, and not leak out the crate arrangement as an impl detail.
pub use vector_config_common::{attributes, validation};

#[doc(hidden)]
pub fn __ensure_numeric_validation_bounds<N>(metadata: &Metadata) -> Result<(), GenerateError>
where
    N: Configurable + ConfigurableNumber,
{
    // In `Validation::ensure_conformance`, we do some checks on any supplied numeric bounds to try and ensure they're
    // no larger than the largest f64 value where integer/floating-point conversions are still lossless.  What we
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
                    return Err(GenerateError::IncompatibleNumericBounds {
                        numeric_type: std::any::type_name::<N>(),
                        bound_direction: BoundDirection::Minimum,
                        mechanical_bound: mechanical_min_bound,
                        specified_bound: *min_bound,
                    });
                }
            }

            if let Some(max_bound) = maximum {
                if *max_bound > mechanical_max_bound {
                    return Err(GenerateError::IncompatibleNumericBounds {
                        numeric_type: std::any::type_name::<N>(),
                        bound_direction: BoundDirection::Maximum,
                        mechanical_bound: mechanical_max_bound,
                        specified_bound: *max_bound,
                    });
                }
            }
        }
    }

    Ok(())
}
