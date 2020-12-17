use crate::{
    expression::{path, Error as ExprErr, Path},
    state, Error as E, Expression, Object, Result, TypeDef, Value,
};

#[derive(thiserror::Error, Clone, Debug, PartialEq)]
pub enum Error {
    #[error(transparent)]
    Query(#[from] path::Error),

    #[error("unknown error: {0}")]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Variable {
    ident: String,
    path: Option<Path>,
}

impl Variable {
    pub fn new(ident: String, path: Option<Path>) -> Self {
        Self { ident, path }
    }

    pub fn boxed(self) -> Box<Self> {
        Box::new(self)
    }

    pub fn ident(&self) -> &str {
        &self.ident
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_ref()
    }
}

impl Expression for Variable {
    fn execute(&self, state: &mut state::Program, _: &mut dyn Object) -> Result<Value> {
        let mut value = state.variable(&self.ident).cloned().unwrap_or(Value::Null);

        if let Some(path) = &self.path {
            return path.execute(state, &mut value).map_err(|err| {
                let err = match err {
                    E::Expression(ExprErr::Path(err)) => Error::Query(err),
                    _ => Error::Unknown(err.to_string()),
                };

                ExprErr::Variable(self.ident.clone(), err).into()
            });
        }

        Ok(value)
    }

    fn type_def(&self, state: &state::Compiler) -> TypeDef {
        state
            .variable_type(&self.ident)
            .cloned()
            // TODO: we can make it so this can never happen, by making it a
            // compile-time error to reference a variable before it is assigned.
            .unwrap_or(TypeDef {
                fallible: true,
                ..Default::default()
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{test_type_def, value::Kind};

    test_type_def![
        ident_match {
            expr: |state: &mut state::Compiler| {
                state.variable_types_mut().insert("foo".to_owned(), TypeDef::default());
                Variable::new("foo".to_owned(), None)
            },
            def: TypeDef::default(),
        }

        exact_match {
            expr: |state: &mut state::Compiler| {
                state.variable_types_mut().insert("foo".to_owned(), TypeDef {
                    fallible: true,
                    kind: Kind::Bytes,
                    ..Default::default()
                });

                Variable::new("foo".to_owned(), None)
            },
            def: TypeDef {
                fallible: true,
                kind: Kind::Bytes,
                ..Default::default()
            },
        }

        ident_mismatch {
            expr: |state: &mut state::Compiler| {
                state.variable_types_mut().insert("foo".to_owned(), TypeDef {
                    fallible: true,
                    ..Default::default()
                });

                Variable::new("bar".to_owned(), None)
            },
            def: TypeDef {
                fallible: true,
                ..Default::default()
            },
        }

        empty_state {
            expr: |_| Variable::new("foo".to_owned(), None),
            def: TypeDef {
                fallible: true,
                ..Default::default()
            },
        }
    ];
}
