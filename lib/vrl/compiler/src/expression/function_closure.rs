use diagnostic::{DiagnosticError, Label};

use crate::expression::Block;
use crate::parser::{Ident, Node};
use crate::Value;
use crate::{value, Context, Expression, ExpressionError};
use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub struct FunctionClosure {
    variables: Vec<Node<Ident>>,
    block: Block,
}

impl FunctionClosure {
    pub(crate) fn new(variables: Vec<Node<Ident>>, block: Block) -> Self {
        Self { variables, block }
    }

    pub(crate) fn variables(&self) -> &[Node<Ident>] {
        &self.variables
    }

    pub fn resolve(
        &self,
        ctx: &mut Context,
        value: Value,
        mut func: impl FnMut(&Context, Output) -> Result<(), ExpressionError>,
    ) -> Result<Value, ExpressionError> {
        match value {
            Value::Object(object) => {
                let key_ident = self
                    .variables
                    .get(0)
                    .expect("checked at compile-time")
                    .clone()
                    .into_inner();
                let value_ident = self
                    .variables
                    .get(1)
                    .expect("checked at compile-time")
                    .clone()
                    .into_inner();

                for (key, value) in object.into_iter() {
                    let state = ctx.state_mut();
                    state.insert_variable(key_ident.clone(), key.into());
                    state.insert_variable(value_ident.clone(), value);

                    let output = match self.block.resolve(ctx)? {
                        Value::Array(mut array) => {
                            let value = match array.pop() {
                                Some(value) => Ok(value),
                                None => Err(Error::ObjectArrayRequired.to_string()),
                            }?;

                            let key = match array.pop() {
                                Some(Value::Bytes(bytes)) => {
                                    Ok(String::from_utf8_lossy(&bytes).into_owned())
                                }
                                None => Err(Error::ObjectArrayRequired.to_string()),
                                _ => Err(Error::ObjectInvalidKey.to_string()),
                            }?;

                            Ok(Output::Object { key, value })
                        }
                        _ => Err(Error::ObjectArrayRequired.to_string()),
                    }?;

                    func(ctx, output)?;

                    let state = ctx.state_mut();
                    state.remove_variable(&key_ident);
                    state.remove_variable(&value_ident);
                }
            }
            _ => unimplemented!(),
        };

        Ok(value!(null))
    }
}

impl fmt::Display for FunctionClosure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // TODO
        self.block.fmt(f)
    }
}

pub enum Output {
    Object { key: String, value: Value },
}

// impl crate::Expression for FunctionClosure {
//     fn resolve(&self, ctx: &mut crate::Context) -> Result<crate::Value, crate::ExpressionError> {
//         Ok(crate::value!(null))
//     }

//     fn type_def(&self, state: &crate::state::Compiler) -> crate::TypeDef {
//         crate::TypeDef::new().null()
//     }
// }

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("object iteration requires a two-element array return value")]
    ObjectArrayRequired,

    #[error("object iteration requires returning a key/value array return value")]
    ObjectNonArray,

    #[error("object iteration requires the first element to be a string type")]
    ObjectInvalidKey,
}

impl DiagnosticError for Error {
    fn code(&self) -> usize {
        0
    }

    fn labels(&self) -> Vec<Label> {
        vec![]
    }
}
