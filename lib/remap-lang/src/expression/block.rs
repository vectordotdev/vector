use crate::{CompilerState, Expr, Expression, Object, Result, State, TypeCheck, Value};

#[derive(Debug, Clone)]
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

    fn type_check(&self, state: &CompilerState) -> TypeCheck {
        self.expressions
            .last()
            .map(|e| e.type_check(state))
            .unwrap_or(TypeCheck {
                optional: true,
                ..Default::default()
            })
    }
}
