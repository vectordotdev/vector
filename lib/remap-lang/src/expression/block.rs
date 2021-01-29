use crate::{state, value, Expr, Expression, Object, Result, TypeDef, Value};

#[derive(Debug, Clone, PartialEq)]
pub struct Block {
    expressions: Vec<Expr>,
}

impl Block {
    pub fn new(expressions: Vec<Expr>) -> Self {
        Self { expressions }
    }
}

impl Expression for Block {
    fn execute(&self, state: &mut state::Program, object: &mut dyn Object) -> Result<Value> {
        self.expressions
            .iter()
            .map(|expr| expr.execute(state, object))
            .collect::<Result<Vec<_>>>()
            .map(|mut v| v.pop().unwrap_or(Value::Null))
    }

    fn type_def(&self, state: &state::Compiler) -> TypeDef {
        let mut type_defs = self
            .expressions
            .iter()
            .map(|e| e.type_def(state))
            .collect::<Vec<_>>();

        // If any of the stored expressions is fallible, the entire block is
        // fallible.
        let fallible = type_defs.iter().any(TypeDef::is_fallible);

        // The last expression determines the resulting value of the block.
        let mut type_def = type_defs
            .pop()
            .unwrap_or_else(|| TypeDef::default().with_constraint(value::Kind::Null));

        type_def.fallible = fallible;
        type_def
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        expression::{Arithmetic, Array, Literal},
        test_type_def,
        type_def::InnerTypeDef,
        value::Kind,
        Operator,
    };

    test_type_def![
        no_expression {
            expr: |_| Block::new(vec![]),
            def: TypeDef {
                kind: Kind::Null,
                ..Default::default()
            },
        }

        one_expression {
            expr: |_| Block::new(vec![Literal::from(true).into()]),
            def: TypeDef { kind: Kind::Boolean, ..Default::default() },
        }

        multiple_expressions {
            expr: |_| Block::new(vec![
                        Literal::from("foo").into(),
                        Literal::from(true).into(),
                        Literal::from(1234).into(),
            ]),
            def: TypeDef { kind: Kind::Integer, ..Default::default() },
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
                kind: Kind::Bytes | Kind::Integer | Kind::Float,
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
                        Array::from(vec![1]).into(),
            ]),
            def: TypeDef {
                fallible: true,
                kind: Kind::Array,
                inner_type_def: Some(InnerTypeDef::Array(TypeDef {
                    fallible: false,
                    kind: Kind::Integer,
                    ..Default::default()
                }.boxed())),
            },
        }
    ];
}
