use crate::{state, value, Expr, Expression, Object, Result, TypeDef, Value};
use std::collections::BTreeMap;
use std::fmt;

#[derive(Clone, PartialEq)]
pub struct Map {
    expressions: BTreeMap<String, Expr>,
}

impl Map {
    pub fn new(expressions: BTreeMap<String, Expr>) -> Self {
        Self { expressions }
    }

    pub fn boxed(self) -> Box<dyn Expression> {
        Box::new(self)
    }
}

impl fmt::Debug for Map {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.expressions.fmt(f)
    }
}

impl From<Map> for BTreeMap<String, Expr> {
    fn from(map: Map) -> Self {
        map.expressions
    }
}

impl<T: Into<Value>> From<BTreeMap<String, T>> for Map {
    fn from(values: BTreeMap<String, T>) -> Self {
        Self::new(
            values
                .into_iter()
                .map(|(k, v)| (k, Expr::from(v)))
                .collect::<BTreeMap<_, _>>(),
        )
    }
}

impl IntoIterator for Map {
    type Item = (String, Expr);
    type IntoIter = std::collections::btree_map::IntoIter<String, Expr>;

    fn into_iter(self) -> Self::IntoIter {
        self.expressions.into_iter()
    }
}

impl Expression for Map {
    fn execute(&self, state: &mut state::Program, object: &mut dyn Object) -> Result<Value> {
        self.expressions
            .iter()
            .map(|(key, expr)| expr.execute(state, object).map(|v| (key.clone(), v)))
            .collect::<Result<BTreeMap<_, _>>>()
            .map(Value::Map)
    }

    fn type_def(&self, state: &state::Compiler) -> TypeDef {
        let fallible = self
            .expressions
            .iter()
            .map(|(_, e)| e.type_def(state))
            .any(|d| d.is_fallible());

        let inner_type_def = if self.expressions.is_empty() {
            None
        } else {
            let type_def = self.expressions.iter().fold(
                TypeDef {
                    kind: value::Kind::empty(),
                    ..Default::default()
                },
                |type_def, (_, expression)| type_def.merge(expression.type_def(state)),
            );

            Some(type_def.boxed())
        };

        TypeDef {
            fallible,
            kind: value::Kind::Map,
            inner_type_def,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{array, expression::Arithmetic, map, test_type_def, value::Kind, Operator};

    test_type_def![
        no_expression {
            expr: |_| map!{},
            def: TypeDef {
                fallible: false,
                kind: Kind::Map,
                ..Default::default()
            },
        }

        one_expression {
            expr: |_| map!{"a": true},
            def: TypeDef {
                kind: Kind::Map,
                inner_type_def: Some(TypeDef {
                    kind: Kind::Boolean,
                    ..Default::default()
                }.boxed()),
                ..Default::default()
            },
        }

        multiple_expressions {
            expr: |_| map!{
                "a": "foo",
                "b": true,
                "c": 1234,
            },
            def: TypeDef {
                kind: Kind::Map,
                inner_type_def: Some(TypeDef {
                    kind: Kind::Bytes | Kind::Boolean | Kind::Integer,
                    ..Default::default()
                }.boxed()),
                ..Default::default()
            },
        }

        last_one_fallible {
            expr: |_| map!{
                "a": value!(true),
                "b": Arithmetic::new(
                    Box::new(value!(12).into()),
                    Box::new(value!(true).into()),
                    Operator::Multiply,
                ),
            },
            def: TypeDef {
                fallible: true,
                kind: Kind::Map,
                inner_type_def: Some(TypeDef {
                    kind: Kind::Bytes | Kind::Integer | Kind::Float | Kind::Boolean,
                    fallible: true,
                    ..Default::default()
                }.boxed()),
            },
        }

        any_fallible {
            expr: |_| map!{
                "a": value!(true),
                "b": Arithmetic::new(
                    Box::new(value!(12).into()),
                    Box::new(value!(true).into()),
                    Operator::Multiply,
                ),
                "c": array![1],
            },
            def: TypeDef {
                fallible: true,
                kind: Kind::Map,
                inner_type_def: Some(TypeDef {
                    kind: Kind::Bytes | Kind::Integer | Kind::Float | Kind::Boolean | Kind::Array,
                    fallible: true,
                    inner_type_def: Some(TypeDef {
                        kind: Kind::Integer,
                        fallible: false,
                        ..Default::default()
                    }.boxed()),
                }.boxed()),
            },
        }
    ];
}
