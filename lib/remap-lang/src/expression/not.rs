use crate::{value, Error as E, Expr, Expression, Object, Result, State, Value};

#[derive(thiserror::Error, Debug, PartialEq)]
pub enum Error {
    #[error("invalid value kind")]
    Value(#[from] value::Error),
}

#[derive(Debug)]
pub(crate) struct Not {
    expression: Box<Expr>,
}

impl Not {
    pub fn new(expression: Box<Expr>) -> Self {
        Self { expression }
    }
}

impl Expression for Not {
    fn execute(&self, state: &mut State, object: &mut dyn Object) -> Result<Option<Value>> {
        self.expression.execute(state, object).and_then(|opt| {
            opt.map(|v| match v {
                Value::Boolean(b) => Ok(Value::Boolean(!b)),
                _ => Err(E::Not(Error::Value(value::Error::Expected(
                    Value::Boolean(true).kind(),
                    v.kind(),
                )))),
            })
            .transpose()
        })
    }
}
