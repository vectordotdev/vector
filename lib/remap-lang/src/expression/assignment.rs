use super::Error as E;
use crate::{CompilerState, Expr, Expression, Object, ResolveKind, Result, State, Value};

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
        let resolve_kind = value.resolves_to(state);

        match &target {
            Target::Variable(ident) => state
                .variable_kinds_mut()
                .insert(ident.clone(), resolve_kind),
            Target::Path(segments) => {
                let path = crate::expression::path::segments_to_path(segments);
                state.variable_kinds_mut().insert(path, resolve_kind)
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

    fn resolves_to(&self, state: &CompilerState) -> ResolveKind {
        match &self.target {
            Target::Variable(ident) => state
                .variable_kind(ident.clone())
                .cloned()
                .unwrap_or(ResolveKind::Any),
            Target::Path(segments) => {
                let path = crate::expression::path::segments_to_path(segments);
                state
                    .variable_kind(&path)
                    .cloned()
                    .unwrap_or(ResolveKind::Any)
            }
        }
    }
}
