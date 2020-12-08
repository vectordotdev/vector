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

        TypeDef {
            fallible,
            kind: value::Kind::Map,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        expression::{Arithmetic, Array, Literal},
        map, test_type_def,
        value::Kind,
        Operator,
    };

    test_type_def![
        no_expression {
            expr: |_| Map::new(map![]),
            def: TypeDef {
                fallible: false,
                kind: Kind::Map,
            },
        }

        one_expression {
            expr: |_| Map::new(map!["a": Expr::from(Literal::from(true))]),
            def: TypeDef { kind: Kind::Map, ..Default::default() },
        }

        multiple_expressions {
            expr: |_| Map::new(map![
                        "a": Expr::from(Literal::from("foo")),
                        "b": Expr::from(Literal::from(true)),
                        "c": Expr::from(Literal::from(1234)),
            ]),
            def: TypeDef { kind: Kind::Map, ..Default::default() },
        }

        last_one_fallible {
            expr: |_| Map::new(map![
                        "a": Expr::from(Literal::from(true)),
                        "b": Expr::from(Arithmetic::new(
                          Box::new(Expr::from(Literal::from(12))),
                          Box::new(Expr::from(Literal::from(true))),
                          Operator::Multiply,
                        )),
            ]),
            def: TypeDef {
                fallible: true,
                kind: Kind::Map,
            },
        }

        any_fallible {
            expr: |_| Map::new(map![
                        "a": Expr::from(Literal::from(true)),
                        "b": Expr::from(Arithmetic::new(
                          Box::new(Expr::from(Literal::from(12))),
                          Box::new(Expr::from(Literal::from(true))),
                          Operator::Multiply,
                        )),
                        "c": Expr::from(Array::from(vec![1])),
            ]),
            def: TypeDef {
                fallible: true,
                kind: Kind::Map,
            },
        }
    ];
}
