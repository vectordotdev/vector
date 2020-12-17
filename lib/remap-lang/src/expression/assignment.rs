use super::Error as E;
use crate::{
    expression::{Path, Variable},
    state, value, Expr, Expression, Object, Result, TypeDef, Value,
};
use std::collections::HashMap;

#[derive(thiserror::Error, Clone, Debug, PartialEq)]
pub enum Error {
    #[error("unable to insert value in path: {0}")]
    PathInsertion(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Target {
    Path(Path),
    Variable(Variable),
    Result {
        variant: TargetResult,
        target: Box<Target>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TargetResult {
    Ok,
    Err,
    Either,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Assignment {
    target: Target,
    value: Box<Expr>,
}

impl Assignment {
    pub fn new(target: Target, value: Box<Expr>, state: &mut state::Compiler) -> Self {
        let type_def = value.type_def(state);

        match &target {
            Target::Result { variant, target } => {
                let mut type_def = type_def.into_fallible(false);

                type_def.kind = match variant {
                    TargetResult::Err => value::Kind::Bytes,
                    TargetResult::Ok => type_def.kind,
                    TargetResult::Either => type_def.kind | value::Kind::Bytes,
                };

                assign_type_def(target.as_ref(), type_def, state)
            }

            _ => assign_type_def(&target, type_def, state),
        };

        Self { target, value }
    }
}

impl Expression for Assignment {
    fn execute(&self, state: &mut state::Program, object: &mut dyn Object) -> Result<Value> {
        let value = self.value.execute(state, object);

        match &self.target {
            Target::Path(path) => {
                let value = value?;

                object
                    .insert(path.as_ref(), value.clone())
                    .map_err(|e| E::Assignment(Error::PathInsertion(e)))?;

                Ok(value.into())
            }

            Target::Variable(variable) => {
                let value = value?;

                state
                    .variables_mut()
                    .insert(variable.ident().to_owned(), value.clone());

                Ok(value.into())
            }

            Target::Result { variant, target } => {
                let result = match variant {
                    TargetResult::Err => value.is_err(),
                    _ => value.is_ok(),
                };

                let value = match variant {
                    TargetResult::Ok if result => value.unwrap(),
                    TargetResult::Err if result => value.unwrap_err().to_string().into(),
                    TargetResult::Either => value.unwrap_or_else(|err| err.to_string().into()),
                    _ => return Ok(result.into()),
                };

                match target.as_ref() {
                    Target::Path(path) => {
                        object
                            .insert(path.as_ref(), value)
                            .map_err(|e| E::Assignment(Error::PathInsertion(e)))?;
                    }

                    Target::Variable(variable) => {
                        state
                            .variables_mut()
                            .insert(variable.ident().to_owned(), value);
                    }

                    _ => unreachable!("cannot assign nested result target"),
                }

                Ok(result.into())
            }
        }
    }

    fn type_def(&self, state: &state::Compiler) -> TypeDef {
        match &self.target {
            Target::Variable(variable) => state
                .variable_type(variable.ident().to_owned())
                .cloned()
                .expect("variable must be assigned via Assignment::new"),
            Target::Path(path) => state
                .path_query_type(path)
                .cloned()
                .expect("path must be assigned via Assignment::new"),
            Target::Result { .. } => TypeDef {
                fallible: false,
                kind: value::Kind::Boolean,
                ..Default::default()
            },
        }
    }
}

fn assign_type_def(target: &Target, type_def: TypeDef, state: &mut state::Compiler) {
    match target {
        Target::Path(path) => assign(
            path.as_ref().clone(),
            type_def,
            state.path_query_types_mut(),
        ),

        Target::Variable(variable) => assign(
            variable.ident().to_owned(),
            type_def,
            state.variable_types_mut(),
        ),

        _ => unreachable!("cannot assign nested result type-def"),
    };
}

fn assign<K, V>(key: K, value: V, state: &mut HashMap<K, V>)
where
    K: Eq + std::hash::Hash,
{
    state.insert(key, value);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{expression::Literal, test_type_def, value::Kind};

    test_type_def![
        variable {
            expr: |state: &mut state::Compiler| {
                let target = Target::Variable(Variable::new("foo".to_owned(), None));
                let value = Box::new(Literal::from(true).into());

                Assignment::new(target, value, state)
            },
            def: TypeDef {
                kind: Kind::Boolean,
                ..Default::default()
            },
        }

        path {
            expr: |state: &mut state::Compiler| {
                let target = Target::Path(Path::from("foo"));
                let value = Box::new(Literal::from("foo").into());

                Assignment::new(target, value, state)
            },
            def: TypeDef {
                kind: Kind::Bytes,
                ..Default::default()
            },
        }
    ];
}
