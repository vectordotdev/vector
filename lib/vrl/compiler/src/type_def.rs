use crate::value::Kind;
use std::ops::{BitAnd, BitOr, BitXor, Deref};

/// Properties for a given expression that express the expected outcome of the
/// expression.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct TypeDef {
    /// True, if an expression can return an error.
    ///
    /// Some expressions are infallible (e.g. the [`Literal`] expression, or any
    /// custom function designed to be infallible).
    pub fallible: bool,

    /// The [`value::Kind`]s this definition represents.
    pub kind: Kind,

    /// Some types contain a collection of other types. If they do, this value
    /// is set to `Some`, and returns the [`TypeDef`] of the collected inner
    /// types.
    pub inner_type_def: Option<Box<TypeDef>>,
}

impl TypeDef {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn empty() -> Self {
        let mut td = Self::default();
        td.kind = Kind::empty();
        td
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
    pub fn bytes(self) -> Self {
        self.scalar(Kind::Bytes)
    }

    #[inline]
    pub fn integer(self) -> Self {
        self.scalar(Kind::Integer)
    }

    #[inline]
    pub fn float(self) -> Self {
        self.scalar(Kind::Float)
    }

    #[inline]
    pub fn boolean(self) -> Self {
        self.scalar(Kind::Boolean)
    }

    #[inline]
    pub fn timestamp(self) -> Self {
        self.scalar(Kind::Timestamp)
    }

    #[inline]
    pub fn regex(self) -> Self {
        self.scalar(Kind::Regex)
    }

    #[inline]
    pub fn null(self) -> Self {
        self.scalar(Kind::Null)
    }

    #[inline]
    pub fn scalar(self, kind: Kind) -> Self {
        debug_assert!(kind.is_scalar());
        self.with_kind(kind).with_inner_type_def(None)
    }

    #[inline]
    pub fn array(self, inner: Option<Self>) -> Self {
        self.container(Kind::Array, inner)
    }

    #[inline]
    pub fn object(self, inner: Option<Self>) -> Self {
        self.container(Kind::Object, inner)
    }

    #[inline]
    pub fn container(self, kind: Kind, inner: Option<Self>) -> Self {
        debug_assert!(!kind.is_scalar());
        self.with_kind(kind).with_inner_type_def(inner)
    }

    #[inline]
    pub fn any(self) -> Self {
        self.with_kind(Kind::all())
    }

    #[inline]
    pub fn with_kind(mut self, kind: Kind) -> Self {
        self.kind = kind;
        self
    }

    #[inline]
    pub fn with_inner_type_def(mut self, inner: Option<Self>) -> Self {
        self.inner_type_def = inner.map(Box::new);
        self
    }

    #[inline]
    pub fn unknown_inner_types(mut self) -> Self {
        self.inner_type_def = None;
        self
    }

    // -------------------------------------------------------------------------

    /// Returns the set of scalar kinds associated with this type definition.
    ///
    /// If a type definition includes an `inner_type_def`, this method will
    /// recursively resolve those until the final scalar kinds are known.
    pub fn scalar_kind(&self) -> Kind {
        let mut kind = self.kind.scalar();
        let mut type_def = self.inner_type_def.clone();

        while let Some(td) = type_def {
            kind |= td.kind.scalar();
            type_def = td.inner_type_def;
        }

        kind
    }

    pub fn is_fallible(&self) -> bool {
        self.fallible
    }

    pub fn is_infallible(&self) -> bool {
        !self.is_fallible()
    }

    /// Returns `true` if the _other_ [`TypeDef`] is contained within the
    /// current one.
    ///
    /// That is to say, its constraints must be more strict or equal to the
    /// constraints of the current one.
    pub fn contains(&self, other: &Self) -> bool {
        // If we don't expect fallible, but the other does, the other's
        // requirement is less strict than ours.
        if !self.is_fallible() && other.is_fallible() {
            return false;
        }

        self.kind.contains(other.kind)
    }

    /// Set the type definition to be fallible if its kind is not contained
    /// within the provided kind.
    pub fn fallible_unless(mut self, kind: impl Into<Kind>) -> Self {
        let kind = kind.into();
        if !kind.contains(self.kind) {
            self.fallible = true
        }

        self
    }

    /// Set the kind constraint for this type definition.
    ///
    /// If the provided kind is a scalar kind, then the inner type definition is
    /// removed from this type definition.
    pub fn with_constraint(mut self, kind: impl Into<Kind>) -> Self {
        self.kind = kind.into();

        if self.kind.is_scalar() {
            self.inner_type_def = None;
        }

        self
    }

    /// Set the inner type definition.
    pub fn with_inner_type(mut self, inner_type: impl Into<Option<Box<Self>>>) -> Self {
        self.inner_type_def = inner_type.into();
        self
    }

    /// Performs a bitwise-or operation, and returns the resulting type definition.
    pub fn merge(self, other: Self) -> Self {
        self | other
    }

    /// Perform a bitwise-or operation, provided the other type definition is
    /// `Some`.
    pub fn merge_optional(self, other: Option<Self>) -> Self {
        match other {
            Some(other) => self.merge(other),
            None => self,
        }
    }
}

impl Default for TypeDef {
    fn default() -> Self {
        Self {
            fallible: false,
            kind: Kind::all(),
            inner_type_def: None,
        }
    }
}

impl Deref for TypeDef {
    type Target = Kind;

    fn deref(&self) -> &Self::Target {
        &self.kind
    }
}

impl BitOr for TypeDef {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self {
        let inner_type_def = match (self.inner_type_def, rhs.inner_type_def) {
            (None, None) => None,
            (lhs @ Some(_), None) => lhs,
            (None, rhs @ Some(_)) => rhs,
            (Some(lhs), Some(rhs)) => Some(Box::new(*lhs | *rhs)),
        };

        Self {
            fallible: self.fallible | rhs.fallible,
            kind: self.kind | rhs.kind,
            inner_type_def,
        }
    }
}

impl BitAnd for TypeDef {
    type Output = Self;

    fn bitand(self, rhs: Self) -> Self::Output {
        let inner_type_def = match (self.inner_type_def, rhs.inner_type_def) {
            (None, None) => None,
            (lhs @ Some(_), None) => lhs,
            (None, rhs @ Some(_)) => rhs,
            (Some(lhs), Some(rhs)) => Some(Box::new(*lhs & *rhs)),
        };

        Self {
            fallible: self.fallible & rhs.fallible,
            kind: self.kind & rhs.kind,
            inner_type_def,
        }
    }
}

impl BitXor for TypeDef {
    type Output = Self;

    fn bitxor(self, rhs: Self) -> Self::Output {
        let inner_type_def = match (self.inner_type_def, rhs.inner_type_def) {
            (None, None) => None,
            (lhs @ Some(_), None) => lhs,
            (None, rhs @ Some(_)) => rhs,
            (Some(lhs), Some(rhs)) => Some(Box::new(*lhs ^ *rhs)),
        };

        Self {
            fallible: self.fallible ^ rhs.fallible,
            kind: self.kind ^ rhs.kind,
            inner_type_def,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scalar_kind() {
        let type_def = TypeDef {
            kind: Kind::Array,
            inner_type_def: Some(Box::new(TypeDef {
                kind: Kind::Boolean | Kind::Float,
                inner_type_def: Some(Box::new(TypeDef {
                    kind: Kind::Bytes,
                    ..Default::default()
                })),
                ..Default::default()
            })),
            ..Default::default()
        };

        assert_eq!(
            type_def.scalar_kind(),
            Kind::Boolean | Kind::Float | Kind::Bytes
        );
    }
}
