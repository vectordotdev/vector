use crate::expression::Block;
use crate::parser::{Ident, Node};
use crate::{Context, Expression, ExpressionError, Value};
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
        mut func: impl FnMut(&Context, Value, Value) -> Result<Value, ExpressionError>,
        mut result: Value,
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

                    let output = self.block.resolve(ctx)?;

                    result = func(ctx, output, result)?;

                    let state = ctx.state_mut();
                    state.remove_variable(&key_ident);
                    state.remove_variable(&value_ident);
                }
                Ok(result)
            }
            Value::Array(array) => {
                let index_ident = self
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

                for (index, value) in array.into_iter().enumerate() {
                    let state = ctx.state_mut();
                    state.insert_variable(index_ident.clone(), index.into());
                    state.insert_variable(value_ident.clone(), value);

                    let output = self.block.resolve(ctx)?;

                    result = func(ctx, output, result)?;

                    let state = ctx.state_mut();
                    state.remove_variable(&index_ident);
                    state.remove_variable(&value_ident);
                }
                Ok(result)
            }
            _ => unimplemented!(),
        }
    }
}

impl fmt::Display for FunctionClosure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // TODO
        f.write_str("{ |")?;

        let mut iter = self.variables.iter().peekable();
        while let Some(var) = iter.next() {
            var.fmt(f)?;

            if iter.peek().is_some() {
                f.write_str(", ")?;
            }
        }

        f.write_str("|\n")?;
        self.block.fmt(f)?;

        f.write_str("\n}")
    }
}

pub enum Output {
    Object { key: String, value: Value },
    Array { element: Value },
}

// impl crate::Expression for FunctionClosure {
//     fn resolve(&self, ctx: &mut crate::Context) -> Result<crate::Value, crate::ExpressionError> {
//         Ok(crate::value!(null))
//     }

//     fn type_def(&self, state: &crate::state::Compiler) -> crate::TypeDef {
//         crate::TypeDef::new().null()
//     }
// }
