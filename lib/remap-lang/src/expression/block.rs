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
        let mut type_checks = self
            .expressions
            .iter()
            .map(|e| e.type_check(state))
            .collect::<Vec<_>>();

        // If any of the stored expressions is fallible, the entire block is
        // fallible.
        let fallible = type_checks.iter().any(TypeCheck::is_fallible);

        // The last expression determines the resulting value of the block.
        let mut type_check = type_checks.pop().unwrap_or(TypeCheck {
            optional: true,
            ..Default::default()
        });

        type_check.fallible = fallible;
        type_check
    }
}
