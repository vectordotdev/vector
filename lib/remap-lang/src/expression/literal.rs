use crate::{state, Expression, Object, Result, TypeDef, Value};

#[derive(Debug, Clone, PartialEq)]
pub struct Literal(Value);

impl Literal {
    pub fn boxed(self) -> Box<dyn Expression> {
        Box::new(self)
    }

    pub fn as_value(&self) -> &Value {
        &self.0
    }
}

impl<T: Into<Value>> From<T> for Literal {
    fn from(value: T) -> Self {
        Self(value.into())
    }
}

impl Expression for Literal {
    fn execute(&self, _: &mut state::Program, _: &mut dyn Object) -> Result<Value> {
        Ok(self.0.clone())
    }

    fn type_def(&self, _: &state::Compiler) -> TypeDef {
        TypeDef {
            kind: self.0.kind(),
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{test_type_def, value::Kind};
    use std::collections::BTreeMap;

    test_type_def![
        boolean {
            expr: |_| Literal::from(true),
            def: TypeDef { kind: Kind::Boolean, ..Default::default() },
        }

        string {
            expr: |_| Literal::from("foo"),
            def: TypeDef { kind: Kind::Bytes, ..Default::default() },
        }

        integer {
            expr: |_| Literal::from(123),
            def: TypeDef { kind: Kind::Integer, ..Default::default() },
        }

        float {
            expr: |_| Literal::from(123.456),
            def: TypeDef { kind: Kind::Float, ..Default::default() },
        }

        array {
            expr: |_| Literal::from(vec!["foo"]),
            def: TypeDef { kind: Kind::Array, ..Default::default() },
        }

        map {
            expr: |_| Literal::from(BTreeMap::default()),
            def: TypeDef { kind: Kind::Map, ..Default::default() },
        }

        timestamp {
            expr: |_| Literal::from(chrono::Utc::now()),
            def: TypeDef { kind: Kind::Timestamp, ..Default::default() },
        }

        null {
            expr: |_| Literal::from(()),
            def: TypeDef { kind: Kind::Null, ..Default::default() },
        }
    ];
}
