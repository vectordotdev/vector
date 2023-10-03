use vrl::value::KeyString;

use crate::Configurable;

/// A string-like type that can be represented in a Vector configuration.
///
/// This is specifically used for constraining the implementation of anything map-like as objects,
/// which maps are represented by, can only have string-like keys.
///
/// If this trait is implemented for a type that is not string-like, things will probably break.
/// Don't implement this for things that are not string-like.
pub trait ConfigurableString: Configurable + ToString {}

impl ConfigurableString for String {}

impl ConfigurableString for KeyString {}
