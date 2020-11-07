use super::Error as E;
use crate::{CompilerState, Expression, Object, Result, State, TypeCheck, Value};

#[derive(thiserror::Error, Debug, PartialEq)]
pub enum Error {
    #[error("undefined variable: {0}")]
    Undefined(String),
}

#[derive(Debug, Clone)]
pub(crate) struct Variable {
    ident: String,
}

impl Variable {
    pub fn new(ident: String) -> Self {
        Self { ident }
    }
}

impl Expression for Variable {
    fn execute(&self, state: &mut State, _: &mut dyn Object) -> Result<Option<Value>> {
        state
            .variable(&self.ident)
            .cloned()
            .ok_or_else(|| E::from(Error::Undefined(self.ident.to_owned())).into())
            .map(Some)
    }

    fn type_check(&self, state: &CompilerState) -> TypeCheck {
        state
            .variable_type(&self.ident)
            .cloned()
            // TODO: we can make it so this can never happen, by making it a
            // compile-time error to reference a variable before it is assigned.
            .unwrap_or(TypeCheck {
                fallible: true,
                ..Default::default()
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{test_type_check, ValueConstraint::*, ValueKind::*};

    test_type_check![
        ident_match {
            expr: |state: &mut CompilerState| {
                state.variable_types_mut().insert("foo".to_owned(), TypeCheck::default());
                Variable::new("foo".to_owned())
            },
            def: TypeCheck::default(),
        }

        exact_match {
            expr: |state: &mut CompilerState| {
                state.variable_types_mut().insert("foo".to_owned(), TypeCheck {
                    fallible: true,
                    optional: false,
                    constraint: Exact(String)
                });

                Variable::new("foo".to_owned())
            },
            def: TypeCheck {
                fallible: true,
                optional: false,
                constraint: Exact(String),
            },
        }

        ident_mismatch {
            expr: |state: &mut CompilerState| {
                state.variable_types_mut().insert("foo".to_owned(), TypeCheck {
                    fallible: true,
                    ..Default::default()
                });

                Variable::new("bar".to_owned())
            },
            def: TypeCheck {
                fallible: true,
                ..Default::default()
            },
        }

        empty_state {
            expr: |_| Variable::new("foo".to_owned()),
            def: TypeCheck {
                fallible: true,
                ..Default::default()
            },
        }
    ];
}
