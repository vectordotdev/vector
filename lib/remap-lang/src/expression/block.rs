use crate::{CompilerState, Expr, Expression, Object, Result, State, TypeDef, Value};

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

    fn type_def(&self, state: &CompilerState) -> TypeDef {
        let mut type_defs = self
            .expressions
            .iter()
            .map(|e| e.type_def(state))
            .collect::<Vec<_>>();

        // If any of the stored expressions is fallible, the entire block is
        // fallible.
        let fallible = type_defs.iter().any(TypeDef::is_fallible);

        // The last expression determines the resulting value of the block.
        let mut type_def = type_defs.pop().unwrap_or(TypeDef {
            optional: true,
            ..Default::default()
        });

        type_def.fallible = fallible;
        type_def
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        expression::Arithmetic, test_type_def, Literal, Operator, ValueConstraint::*, ValueKind::*,
    };

    test_type_def![
        no_expression {
            expr: |_| Block::new(vec![]),
            def: TypeDef { optional: true, ..Default::default() },
        }

        one_expression {
            expr: |_| Block::new(vec![Literal::from(true).into()]),
            def: TypeDef { constraint: Exact(Boolean), ..Default::default() },
        }

        multiple_expressions {
            expr: |_| Block::new(vec![
                        Literal::from("foo").into(),
                        Literal::from(true).into(),
                        Literal::from(1234).into(),
            ]),
            def: TypeDef { constraint: Exact(Integer), ..Default::default() },
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
            def: TypeDef {
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
            def: TypeDef {
                fallible: true,
                constraint: Exact(Array),
                ..Default::default()
            },
        }
    ];
}
