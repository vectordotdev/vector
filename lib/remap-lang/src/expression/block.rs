use crate::{Expr, Expression, Object, Result, State, Value};

#[derive(Debug)]
pub(crate) struct Block {
    expressions: Vec<Expr>,
}

impl Block {
    pub fn new(expressions: Vec<Expr>) -> Self {
        Self { expressions }
    }
}

impl Expression for Block {
    fn execute(&self, state: &mut State, object: &mut dyn Object) -> Result<Option<Value>> {
        let mut value = None;

        for expr in &self.expressions {
            value = expr.execute(state, object)?;
        }

        Ok(value)
    }
}
