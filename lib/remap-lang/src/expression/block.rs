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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        expression::Arithmetic, test_type_check, Literal, Operator, ValueConstraint::*,
        ValueKind::*,
    };

    test_type_check![
        no_expression {
            expr: |_| Block::new(vec![]),
            def: TypeCheck { optional: true, ..Default::default() },
        }

        one_expression {
            expr: |_| Block::new(vec![Literal::from(true).into()]),
            def: TypeCheck { constraint: Exact(Boolean), ..Default::default() },
        }

        multiple_expressions {
            expr: |_| Block::new(vec![
                        Literal::from("foo").into(),
                        Literal::from(true).into(),
                        Literal::from(1234).into(),
            ]),
            def: TypeCheck { constraint: Exact(Integer), ..Default::default() },
        }

        last_one_fallible {
            expr: |_| Block::new(vec![
                        Literal::from(true).into(),
                        Arithmetic::new(
                          Box::new(Literal::from(12).into()),
                          Box::new(Literal::from(true).into()),
                          Operator::Multiply,
                        ).into(),
            ]),
            def: TypeCheck {
                fallible: true,
                constraint: OneOf(vec![String, Integer, Float]),
                ..Default::default()
            },
        }

        any_fallible {
            expr: |_| Block::new(vec![
                        Literal::from(true).into(),
                        Arithmetic::new(
                          Box::new(Literal::from(12).into()),
                          Box::new(Literal::from(true).into()),
                          Operator::Multiply,
                        ).into(),
                        Literal::from(vec![1]).into(),
            ]),
            def: TypeCheck {
                fallible: true,
                constraint: Exact(Array),
                ..Default::default()
            },
        }
    ];
}
