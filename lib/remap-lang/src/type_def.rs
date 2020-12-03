use crate::value;
use std::ops::{BitAnd, BitOr, BitXor};

/// Properties for a given expression that express the expected outcome of the
/// expression.
///
/// This includes whether the expression is fallible, whether it can return
/// "nothing", and a list of values the expression can resolve to.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct TypeDef {
    /// True, if an expression can return an error.
    ///
    /// Some expressions are infallible (e.g. the [`Literal`] expression, or any
    /// custom function designed to be infallible).
    pub fallible: bool,

    /// The [`value::Kind`]s this definition represents.
    pub kind: value::Kind,

    /// Some types contain a collection of other types. If they do, this value
    /// is set to `Some`, and returns the [`TypeDef`] of the collected inner
    /// types.
    ///
    /// For example, given a [`Value::Array`]:
    ///
    /// ```rust
    /// # use remap_lang::{expression::Array, Value, Expression, state, TypeDef, value::Kind};
    ///
    /// let vec = vec![Value::Null, Value::Boolean(true)];
    /// let expression = Array::from(vec);
    /// let state = state::Compiler::default();
    ///
    /// assert_eq!(
    ///     expression.type_def(&state),
    ///     TypeDef {
    ///         fallible: false,
    ///         kind: Kind::Array,
    ///         inner_type_def: Some(TypeDef {
    ///             fallible: false,
    ///             kind: Kind::Null | Kind::Boolean,
    ///             inner_type_def: None,
    ///         }.boxed()),
    ///     },
    /// );
    /// ```
    pub inner_type_def: Option<Box<TypeDef>>,
}

impl Default for TypeDef {
    fn default() -> Self {
        Self {
            fallible: false,
            kind: value::Kind::all(),
            inner_type_def: None,
        }
    }
}

impl BitOr for TypeDef {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self {
        let inner_type_def = match (self.inner_type_def, rhs.inner_type_def) {
            (None, None) => None,
            (lhs @ Some(_), None) => lhs,
            (None, rhs @ Some(_)) => rhs,
            (Some(lhs), Some(rhs)) => Some((*lhs | *rhs).boxed()),
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
            (Some(lhs), Some(rhs)) => Some((*lhs & *rhs).boxed()),
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
            (Some(lhs), Some(rhs)) => Some((*lhs ^ *rhs).boxed()),
        };

        Self {
            fallible: self.fallible ^ rhs.fallible,
            kind: self.kind ^ rhs.kind,
            inner_type_def,
        }
    }
}

impl TypeDef {
    pub fn boxed(self) -> Box<Self> {
        Box::new(self)
    }

    pub fn is_fallible(&self) -> bool {
        self.fallible
    }

    pub fn into_fallible(mut self, fallible: bool) -> Self {
        self.fallible = fallible;
        self
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

    pub fn fallible_unless(mut self, kind: impl Into<value::Kind>) -> Self {
        if !kind.into().contains(self.kind) {
            self.fallible = true
        }

        self
    }

    pub fn with_constraint(mut self, kind: impl Into<value::Kind>) -> Self {
        self.kind = kind.into();
        self
    }

    pub fn merge(self, other: Self) -> Self {
        self | other
    }

    pub fn merge_optional(self, other: Option<Self>) -> Self {
        match other {
            Some(other) => self.merge(other),
            None => self,
        }
    }

    /// Similar to `merge_optional`, except that the optional `TypeDef` is
    /// considered to be the "default" for the `self` `TypeDef`.
    ///
    /// The implication of this is that the resulting `TypeDef` will be equal to
    /// `self` or `other`, if either of the two is infallible.
    ///
    /// If neither are, the two type definitions are merged as usual.
    pub fn merge_with_default_optional(self, other: Option<Self>) -> Self {
        if !self.is_fallible() {
            return self;
        }

        match other {
            None => self,

            // If `self` isn't exact, see if `other` is.
            Some(other) if !other.is_fallible() => other,

            // Otherwise merge the optional as usual.
            Some(other) => self.merge(other),
        }
    }
}
