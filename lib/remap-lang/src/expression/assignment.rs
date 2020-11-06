use super::Error as E;
use crate::{CompilerState, Expr, Expression, Object, Result, State, TypeCheck, Value};

#[derive(thiserror::Error, Debug, PartialEq)]
pub enum Error {
    #[error("unable to insert value in path: {0}")]
    PathInsertion(String),
}

#[derive(Debug, Clone)]
pub(crate) enum Target {
    Path(Vec<Vec<String>>),
    Variable(String),
}

#[derive(Debug, Clone)]
pub(crate) struct Assignment {
    target: Target,
    value: Box<Expr>,
}

impl Assignment {
    pub fn new(target: Target, value: Box<Expr>, state: &mut CompilerState) -> Self {
        let type_check = value.type_check(state);

        match &target {
            Target::Variable(ident) => state.variable_types_mut().insert(ident.clone(), type_check),
            Target::Path(segments) => {
                let path = crate::expression::path::segments_to_path(segments);
                state.path_query_types_mut().insert(path, type_check)
            }
        };

        Self { target, value }
    }
}

impl Expression for Assignment {
    fn execute(&self, state: &mut State, object: &mut dyn Object) -> Result<Option<Value>> {
        let value = self.value.execute(state, object)?;

        match value {
            None => Ok(None),
            Some(value) => {
                match &self.target {
                    Target::Variable(ident) => {
                        state.variables_mut().insert(ident.clone(), value.clone());
                    }
                    Target::Path(path) => object
                        .insert(&path, value.clone())
                        .map_err(|e| E::Assignment(Error::PathInsertion(e)))?,
                }

                Ok(Some(value))
            }
        }
    }

    fn type_check(&self, state: &CompilerState) -> TypeCheck {
        match &self.target {
            Target::Variable(ident) => state
                .variable_type(ident.clone())
                .cloned()
                // TODO: we can make it so this can never happen, by making it a
                // compile-time error to reference a variable before it is assigned.
                .unwrap_or_default(),
            Target::Path(segments) => {
                let path = crate::expression::path::segments_to_path(segments);
                state.path_query_type(&path).cloned().unwrap_or_default()
            }
        }
    }
}
