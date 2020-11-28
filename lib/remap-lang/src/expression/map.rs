use crate::{state, value, Expr, Expression, Object, Result, TypeDef, Value};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq)]
pub struct Map {
    expressions: BTreeMap<String, Expr>,
}

impl Map {
    pub fn new(expressions: BTreeMap<String, Expr>) -> Self {
        Self { expressions }
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
