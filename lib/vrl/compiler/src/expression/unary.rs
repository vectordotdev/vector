use std::fmt;

use crate::{
    expression::{Not, Resolved},
    state::{TypeInfo, TypeState},
    Context, Expression,
};

#[derive(Debug, Clone, PartialEq)]
pub struct Unary {
    variant: Variant,
}

impl Unary {
    #[must_use]
    pub fn new(variant: Variant) -> Self {
        Self { variant }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Variant {
    Not(Not),
}

impl Expression for Unary {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        use Variant::Not;

        match &self.variant {
            Not(v) => v.resolve(ctx),
        }
    }

    fn type_info(&self, state: &TypeState) -> TypeInfo {
        use Variant::Not;

        let mut state = state.clone();

        let result = match &self.variant {
            Not(v) => v.apply_type_info(&mut state),
        };
        TypeInfo::new(state, result)
    }
}

impl fmt::Display for Unary {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use Variant::Not;

        match &self.variant {
            Not(v) => v.fmt(f),
        }
    }
}

impl From<Not> for Variant {
    fn from(not: Not) -> Self {
        Variant::Not(not)
    }
}
