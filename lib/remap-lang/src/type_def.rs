use crate::value;

/// Properties for a given expression that express the expected outcome of the
/// expression.
///
/// This includes whether the expression is fallible, whether it can return
/// "nothing", and a list of values the expression can resolve to.
#[derive(Debug, Clone, Eq, PartialEq, Default)]
pub struct TypeDef {
    /// True, if an expression can return an error.
    ///
    /// Some expressions are infallible (e.g. the [`Literal`] expression, or any
    /// custom function designed to be infallible).
    pub fallible: bool,

    /// True, if an expression can resolve to "nothing".
    ///
    /// For example, and if-statement without an else-condition can resolve to
    /// nothing if the if-condition does not match.
    pub optional: bool,

    /// The [`value::Constraint`] applied to this type check.
    ///
    /// This resolves to a list of [`value::Kind`]s the expression is expected
    /// to return.
    pub constraint: value::Constraint,
}

impl TypeDef {
    pub fn is_fallible(&self) -> bool {
        self.fallible
    }

    pub fn into_fallible(mut self, fallible: bool) -> Self {
        self.fallible = fallible;
        self
    }

    pub fn is_optional(&self) -> bool {
        self.optional
    }

    pub fn into_optional(mut self, optional: bool) -> Self {
        self.optional = optional;
        self
    }

    /// Returns `true` if the _other_ [`TypeDef`] is contained within the
    /// current one.
    ///
    /// That is to say, its constraints must be more strict or equal to the
    /// constraints of the current one.
    pub fn contains(&self, other: &Self) -> bool {
        // If we don't expect none, but the other does, the other's requirement
        // is less strict than ours.
        if !self.is_optional() && other.is_optional() {
            return false;
        }

        // The same applies to fallible checks.
        if !self.is_fallible() && other.is_fallible() {
            return false;
        }

        self.constraint.contains(&other.constraint)
    }

    pub fn fallible_unless(mut self, constraint: impl Into<value::Constraint>) -> Self {
        if !constraint.into().contains(&self.constraint) {
            self.fallible = true
        }

        self
    }

    pub fn with_constraint(mut self, constraint: impl Into<value::Constraint>) -> Self {
        self.constraint = constraint.into();
        self
    }

    pub fn merge(self, other: Self) -> Self {
        let TypeDef {
            fallible,
            optional,
            constraint,
        } = other;

        // TODO: take `self`
        let constraint = self.constraint.merge(&constraint);

        Self {
            fallible: self.is_fallible() || fallible,
            optional: self.is_optional() || optional,
            constraint,
        }
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
    /// `self` or `other`, if either of the two is infallible and non-optional.
    ///
    /// If neither are, the two type definitions are merged as usual.
    pub fn merge_with_default_optional(self, other: Option<Self>) -> Self {
        if !self.is_fallible() && !self.is_optional() {
            return self;
        }

        match other {
            None => self,

            // If `self` isn't exact, see if `other` is.
            Some(other) if !other.is_fallible() && !other.is_optional() => other,

            // Otherwise merge the optional as usual.
            Some(other) => self.merge(other),
        }
    }
}
