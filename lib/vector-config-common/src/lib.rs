// TODO: `darling` is currently strict about accepting only matching literal types for scalar fields i.e. a `f64` field
// can only be parsed from a string or float literal, but not an integer literal... and float literals have to be in the
// form of `1000.0`, not `1000`.
//
// This means we need to use float numbers for range validation if the field it's applied to is an integer.. which is
// not great from a UX perspective.  `darling` lacks the ability to incrementally parse a field to avoid having to
// expose a custom type that gets used downstream...

#![deny(warnings)]
pub mod num;
pub mod validation;
