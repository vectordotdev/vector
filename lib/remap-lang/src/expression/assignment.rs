use super::Error as E;
use crate::{CompilerState, Expr, Expression, Object, Result, State, TypeDef, Value};

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
        let type_def = value.type_def(state);

        match &target {
            Target::Variable(ident) => state.variable_types_mut().insert(ident.clone(), type_def),
            Target::Path(segments) => {
                let path = crate::expression::path::segments_to_path(segments);
                state.path_query_types_mut().insert(path, type_def)
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

    fn type_def(&self, state: &CompilerState) -> TypeDef {
        match &self.target {
            Target::Variable(ident) => state
                .variable_type(ident.clone())
                .cloned()
                .expect("variable must be assigned via Assignment::new"),
            Target::Path(segments) => {
                let path = crate::expression::path::segments_to_path(segments);
                state
                    .path_query_type(&path)
                    .cloned()
                    .expect("variable must be assigned via Assignment::new")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{test_type_def, Literal, ValueConstraint::*, ValueKind::*};

    test_type_def![
        variable {
            expr: |state: &mut CompilerState| {
                let target = Target::Variable("foo".to_owned());
                let value = Box::new(Literal::from(true).into());

                Assignment::new(target, value, state)
            },
            def: TypeDef {
                constraint: Exact(Boolean),
                ..Default::default()
            },
        }

        path {
            expr: |state: &mut CompilerState| {
                let target = Target::Path(vec![vec!["foo".to_owned()]]);
                let value = Box::new(Literal::from("foo").into());

                Assignment::new(target, value, state)
            },
            def: TypeDef {
                constraint: Exact(String),
                ..Default::default()
            },
        }
    ];
}
