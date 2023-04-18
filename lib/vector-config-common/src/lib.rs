// TODO: `darling` is currently strict about accepting only matching literal types for scalar fields i.e. a `f64` field
// can only be parsed from a string or float literal, but not an integer literal... and float literals have to be in the
// form of `1000.0`, not `1000`.
//
// This means we need to use float numbers for range validation if the field it's applied to is an integer.. which is
// not great from a UX perspective.  `darling` lacks the ability to incrementally parse a field to avoid having to
// expose a custom type that gets used downstream...
//
// TODO: we should add a shorthand validator for "not empty". right now, for strings, we have to say
// `#[configurable(validation(length(min = 1)))]` to indicate the string cannot be empty, when
// something like `#[configurable(validation(not_empty)]` is a bit more self-evident, and shorter to boot

#![deny(warnings)]
pub mod attributes;
pub mod constants;
pub mod human_friendly;
pub mod num;
pub mod schema;
pub mod validation;
