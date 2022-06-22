//! TypeDefs
//!
//! The type definitions for typedefs record the various possible type definitions for the state
//! that can be passed through a VRL program.
//!
//! `TypeDef` contains a `KindInfo`.
//!
//! `KindInfo` can be:
//! `Unknown` - We don't know what type this is.
//! `Known` - A set of the possible known `TypeKind`s. There can be multiple possible types for a
//! path in scenarios such as `if .thing { .x = "hello" } else { .x = 42 }`. In that example after
//! that statement is run, `.x` could contain either an string or an integer, we won't know until
//! runtime exactly which.
//!
//! `TypeKind` is a concrete type for a path, `Bytes` (string), `Integer`, `Float`, `Boolean`,
//! `Timestamp`, `Regex`, `Null` or `Array` or `Object`.
//!
//! `Array` is a Map of `Index` -> `KindInfo`.
//! `Index` can be a specific index into that array, or `Any` which represents any index found within
//! that array.
//!
//! `Object` is a Map of `Field` -> `KindInfo`.
//! `Field` can be a specifix field name of the object, or `Any` which represents any element found
//! within that object.

use std::{
    borrow::Cow,
    ops::{Deref, DerefMut},
};

use lookup::Lookup;
use value::{
    kind::{
        merge,
        nest::{CoalescedPath, Strategy},
        Collection, Field, Index,
    },
    Kind, Value,
};

/// Properties for a given expression that express the expected outcome of the
/// expression.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct TypeDef {
    /// True, if an expression can return an error.
    ///
    /// Some expressions are infallible (e.g. the [`Literal`][crate::expression::Literal] expression, or any
    /// custom function designed to be infallible).
    fallible: bool,

    /// The [`Kind`][value::Kind]s this definition represents.
    kind: Kind,
}

impl Deref for TypeDef {
    type Target = Kind;

    fn deref(&self) -> &Self::Target {
        &self.kind
    }
}

impl DerefMut for TypeDef {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.kind
    }
}

impl TypeDef {
    pub fn kind(&self) -> &Kind {
        &self.kind
    }

    pub fn at_path(&self, path: &Lookup<'_>) -> TypeDef {
        let fallible = self.fallible;

        let kind = self
            .kind
            .find_at_path(path)
            .ok()
            .flatten()
            .map(Cow::into_owned)
            .unwrap_or_else(Kind::any);

        Self { fallible, kind }
    }

    pub fn for_path(self, path: &Lookup<'_>) -> TypeDef {
        let fallible = self.fallible;
        let kind = self
            .kind
            .clone()
            .nest_at_path(
                path,
                Strategy {
                    coalesced_path: CoalescedPath::Reject,
                },
            )
            .unwrap_or(self.kind);

        Self { fallible, kind }
    }

    #[inline]
    pub fn fallible(mut self) -> Self {
        self.fallible = true;
        self
    }

    #[inline]
    pub fn infallible(mut self) -> Self {
        self.fallible = false;
        self
    }

    #[inline]
    pub fn with_fallibility(mut self, fallible: bool) -> Self {
        self.fallible = fallible;
        self
    }

    #[inline]
    pub fn any() -> Self {
        Kind::any().into()
    }

    #[inline]
    pub fn bytes() -> Self {
        Kind::bytes().into()
    }

    #[inline]
    pub fn add_bytes(mut self) -> Self {
        self.kind.add_bytes();
        self
    }

    #[inline]
    pub fn integer() -> Self {
        Kind::integer().into()
    }

    #[inline]
    pub fn add_integer(mut self) -> Self {
        self.kind.add_integer();
        self
    }

    #[inline]
    pub fn float() -> Self {
        Kind::float().into()
    }

    #[inline]
    pub fn add_float(mut self) -> Self {
        self.kind.add_float();
        self
    }

    #[inline]
    pub fn boolean() -> Self {
        Kind::boolean().into()
    }

    #[inline]
    pub fn add_boolean(mut self) -> Self {
        self.kind.add_boolean();
        self
    }

    #[inline]
    pub fn timestamp() -> Self {
        Kind::timestamp().into()
    }

    #[inline]
    pub fn add_timestamp(mut self) -> Self {
        self.kind.add_timestamp();
        self
    }

    #[inline]
    pub fn regex() -> Self {
        Kind::regex().into()
    }

    #[inline]
    pub fn add_regex(mut self) -> Self {
        self.kind.add_regex();
        self
    }

    #[inline]
    pub fn null() -> Self {
        Kind::null().into()
    }

    #[inline]
    pub fn never() -> Self {
        Kind::never().into()
    }

    #[inline]
    pub fn add_null(mut self) -> Self {
        self.kind.add_null();
        self
    }

    #[inline]
    pub fn array(collection: impl Into<Collection<Index>>) -> Self {
        Kind::array(collection).into()
    }

    #[inline]
    pub fn add_array(mut self, collection: impl Into<Collection<Index>>) -> Self {
        self.kind.add_array(collection);
        self
    }

    /// Convert the [`TypeDef`]s [`Kind`] to an array.
    ///
    /// If `Kind` already has the array state, all other states are removed. If it does not yet
    /// have an array, then equally all existing states are removed, and an "any" array state is
    /// added.
    ///
    /// `TypeDef`s fallibility is kept unmodified.
    #[inline]
    pub fn restrict_array(self) -> Self {
        let fallible = self.fallible;
        let collection = match self.kind.into_array() {
            Some(array) => array,
            None => Collection::any(),
        };

        Self {
            fallible,
            kind: Kind::array(collection),
        }
    }

    #[inline]
    pub fn object(collection: impl Into<Collection<Field>>) -> Self {
        Kind::object(collection).into()
    }

    #[inline]
    pub fn add_object(mut self, collection: impl Into<Collection<Field>>) -> Self {
        self.kind.add_object(collection);
        self
    }

    /// Convert the [`TypeDef`]s [`Kind`] to an object.
    ///
    /// If `Kind` already has the object state, all other states are removed. If it does not yet
    /// have an object, then equally all existing states are removed, and an "any" object state is
    /// added.
    ///
    /// `TypeDef`s fallibility is kept unmodified.
    #[inline]
    pub fn restrict_object(self) -> Self {
        let fallible = self.fallible;
        let collection = match self.kind.into_object() {
            Some(object) => object,
            None => Collection::any(),
        };

        Self {
            fallible,
            kind: Kind::object(collection),
        }
    }

    #[inline]
    pub fn with_kind(mut self, kind: Kind) -> Self {
        self.kind = kind;
        self
    }

    /// Collects any subtypes that can contain multiple indexed types (array, object) and collects
    /// them into a single type for all indexes.
    ///
    /// Used for functions that cant determine which indexes of a collection have been used in the
    /// result.
    pub fn collect_subtypes(mut self) -> Self {
        if let Some(object) = self.kind.as_object_mut() {
            object.set_unknown(None);
            object.anonymize();
        }
        if let Some(array) = self.kind.as_array_mut() {
            array.set_unknown(None);
            array.anonymize();
        }

        self
    }

    // -------------------------------------------------------------------------

    pub fn is_fallible(&self) -> bool {
        self.fallible
    }

    pub fn is_infallible(&self) -> bool {
        !self.is_fallible()
    }

    /// Set the type definition to be fallible if its kind is not contained
    /// within the provided kind.
    pub fn fallible_unless(mut self, kind: impl Into<Kind>) -> Self {
        let kind = kind.into();
        if !kind.is_superset(&self.kind) {
            self.fallible = true
        }

        self
    }

    pub fn merge_deep(mut self, other: Self) -> Self {
        self.merge(
            other,
            merge::Strategy {
                collisions: merge::CollisionStrategy::Union,
                indices: merge::Indices::Keep,
            },
        );
        self
    }

    /// Merge two type definitions.
    ///
    /// When merging arrays, the elements of `other` are *appended* to the elements of `self`.
    /// Meaning, the indices of `other` are updated, to continue onward from the last index of
    /// `self`.
    pub fn merge_append(mut self, other: Self) -> Self {
        self.merge(
            other,
            merge::Strategy {
                collisions: merge::CollisionStrategy::Overwrite,
                indices: merge::Indices::Append,
            },
        );
        self
    }

    pub fn merge(&mut self, other: Self, strategy: merge::Strategy) {
        self.fallible |= other.fallible;
        self.kind.merge(other.kind, strategy);
    }

    pub fn with_type_set_at_path(self, path: &Lookup, other: Self) -> Self {
        if path.is_root() {
            other
        } else {
            self.merge_overwrite(other.for_path(path))
        }
    }

    pub fn merge_overwrite(mut self, other: Self) -> Self {
        self.merge(
            other,
            merge::Strategy {
                collisions: merge::CollisionStrategy::Overwrite,
                indices: merge::Indices::Keep,
            },
        );
        self
    }
}

impl From<Kind> for TypeDef {
    fn from(kind: Kind) -> Self {
        Self {
            fallible: false,
            kind,
        }
    }
}

impl From<TypeDef> for Kind {
    fn from(type_def: TypeDef) -> Self {
        type_def.kind
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Details {
    pub(crate) type_def: TypeDef,
    pub(crate) value: Option<Value>,
}

impl Details {
    /// Returns the union of 2 possible states
    pub(crate) fn merge(self, other: Self) -> Self {
        Self {
            type_def: self.type_def.merge_deep(other.type_def),
            value: if self.value == other.value {
                self.value
            } else {
                None
            },
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn merge_details_same_literal() {
        let a = Details {
            type_def: TypeDef::integer(),
            value: Some(Value::from(5)),
        };
        let b = Details {
            type_def: TypeDef::float(),
            value: Some(Value::from(5)),
        };
        assert_eq!(
            a.merge(b),
            Details {
                type_def: TypeDef::integer().add_float(),
                value: Some(Value::from(5))
            }
        )
    }

    #[test]
    fn merge_details_different_literal() {
        let a = Details {
            type_def: TypeDef::any(),
            value: Some(Value::from(5)),
        };
        let b = Details {
            type_def: TypeDef::object(Collection::empty()),
            value: Some(Value::from(6)),
        };
        assert_eq!(
            a.merge(b),
            Details {
                type_def: TypeDef::any(),
                value: None
            }
        )
    }
}
