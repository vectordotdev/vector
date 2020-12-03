use crate::{state, value, Expr, Expression, Object, Result, TypeDef, Value};
use std::fmt;
use std::iter::IntoIterator;

#[derive(Clone, PartialEq)]
pub struct Array {
    expressions: Vec<Expr>,
}

impl Array {
    pub fn new(expressions: Vec<Expr>) -> Self {
        Self { expressions }
    }
}

impl fmt::Debug for Array {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.expressions.fmt(f)
    }
}

impl IntoIterator for Array {
    type Item = Expr;
    type IntoIter = std::vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        self.expressions.into_iter()
    }
}

impl Expression for Array {
    fn execute(&self, state: &mut state::Program, object: &mut dyn Object) -> Result<Value> {
        self.expressions
            .iter()
            .map(|expr| expr.execute(state, object))
            .collect::<Result<Vec<_>>>()
            .map(Value::Array)
    }

    fn type_def(&self, state: &state::Compiler) -> TypeDef {
        let fallible = self
            .expressions
            .iter()
            .map(|e| e.type_def(state))
            .any(|d| d.is_fallible());

        TypeDef {
            fallible,
            kind: value::Kind::Array,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        expression::{Arithmetic, Literal},
        test_type_def,
        value::Kind,
        Operator,
    };

    test_type_def![
        no_expression {
            expr: |_| Array::new(vec![]),
            def: TypeDef {
                fallible: false,
                kind: Kind::Array,
            },
        }

        one_expression {
            expr: |_| Array::new(vec![Literal::from(true).into()]),
            def: TypeDef { kind: Kind::Array, ..Default::default() },
        }

        multiple_expressions {
            expr: |_| Array::new(vec![
                        Literal::from("foo").into(),
                        Literal::from(true).into(),
                        Literal::from(1234).into(),
            ]),
            def: TypeDef { kind: Kind::Array, ..Default::default() },
        }

        last_one_fallible {
            expr: |_| Array::new(vec![
                        Literal::from(true).into(),
                        Arithmetic::new(
                          Box::new(Literal::from(12).into()),
                          Box::new(Literal::from(true).into()),
                          Operator::Multiply,
                        ).into(),
            ]),
            def: TypeDef {
                fallible: true,
                kind: Kind::Array,
            },
        }

        any_fallible {
            expr: |_| Array::new(vec![
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
                kind: Kind::Array,
            },
        }
    ];
}
