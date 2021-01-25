use crate::{
    expression::Literal, state, value, Error, Expr, Expression, Object, Result, TypeDef, Value,
};
use std::convert::TryFrom;
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

    pub fn boxed(self) -> Box<dyn Expression> {
        Box::new(self)
    }

    /// Unwrap the array expression into a [`Value::Array`] type.
    ///
    /// This method panics if the stored expressions do not resolve to a literal
    /// value.
    pub fn unwrap_value(self) -> Value {
        Value::try_from(self).expect("array includes non-literal expressions")
    }
}

impl fmt::Debug for Array {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.expressions.fmt(f)
    }
}

impl From<Array> for Vec<Expr> {
    fn from(array: Array) -> Self {
        array.expressions
    }
}

impl<T: Into<Value>> From<Vec<T>> for Array {
    fn from(values: Vec<T>) -> Self {
        Self::new(
            values
                .into_iter()
                .map(Into::into)
                .map(Expr::from)
                .collect::<Vec<_>>(),
        )
    }
}

impl TryFrom<Value> for Array {
    type Error = Error;

    fn try_from(value: Value) -> Result<Array> {
        match value {
            Value::Array(array) => Ok(array.into()),
            v => Err(Error::Value(value::Error::Expected(
                value::Kind::Array,
                v.kind(),
            ))),
        }
    }
}

impl TryFrom<Array> for Value {
    type Error = Error;

    fn try_from(array: Array) -> Result<Value> {
        array
            .into_iter()
            .map(|expr| match expr {
                Expr::Array(v) => Value::try_from(v),
                _ => Literal::try_from(expr)
                    .map(|v| v.into_value())
                    .map_err(Into::into),
            })
            .collect::<Result<Vec<_>>>()
            .map(|v| v.into())
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

        let inner_type_def = if self.expressions.is_empty() {
            None
        } else {
            let type_def = self.expressions.iter().fold(
                TypeDef {
                    kind: value::Kind::empty(),
                    ..Default::default()
                },
                |type_def, expression| type_def.merge(expression.type_def(state)),
            );

            Some(type_def.boxed())
        };

        TypeDef {
            fallible,
            kind: value::Kind::Array,
            inner_type_def,
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
                ..Default::default()
            },
        }

        one_expression {
            expr: |_| Array::new(vec![Literal::from(true).into()]),
            def: TypeDef {
                kind: Kind::Array,
                inner_type_def: Some(TypeDef {
                    kind: Kind::Boolean,
                    ..Default::default()
                }.boxed()),
                ..Default::default()
            },
        }

        multiple_expressions {
            expr: |_| Array::new(vec![
                        Literal::from("foo").into(),
                        Literal::from(true).into(),
                        Literal::from(1234).into(),
            ]),
            def: TypeDef {
                kind: Kind::Array,
                inner_type_def: Some(TypeDef {
                    kind: Kind::Bytes | Kind::Boolean | Kind::Integer,
                    ..Default::default()
                }.boxed()),
                ..Default::default()
            },
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
                inner_type_def: Some(TypeDef {
                    fallible: true,
                    kind: Kind::Boolean | Kind::Integer | Kind::Float | Kind::Bytes,
                    ..Default::default()
                }.boxed()),
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
                        Array::from(vec![1]).into(),
            ]),
            def: TypeDef {
                fallible: true,
                kind: Kind::Array,
                inner_type_def: Some(TypeDef {
                    fallible: true,
                    kind: Kind::Boolean | Kind::Integer | Kind::Float | Kind::Bytes | Kind::Boolean | Kind::Array,
                    inner_type_def: Some(TypeDef {
                        kind: Kind::Integer,
                        ..Default::default()
                    }.boxed()),
                }.boxed()),
            },
        }
    ];
}
