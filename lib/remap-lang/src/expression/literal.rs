use crate::{state, Expression, Object, Result, TypeDef, Value};
use std::fmt;
use std::ops::Deref;

#[derive(Clone, PartialEq)]
pub struct Literal(Value);

impl Literal {
    pub fn new(value: Value) -> Self {
        debug_assert!(
            !matches!(value, Value::Array(_)),
            "{} must use expression::Array instead of expression::Literal",
            value.kind()
        );

        debug_assert!(
            !matches!(value, Value::Map(_)),
            "{} must use expression::Map instead of expression::Literal",
            value.kind()
        );

        Self(value)
    }

    pub fn boxed(self) -> Box<dyn Expression> {
        Box::new(self)
    }

    pub fn as_value(&self) -> &Value {
        &self.0
    }

    pub fn into_value(self) -> Value {
        self.0
    }
}

impl fmt::Debug for Literal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl Deref for Literal {
    type Target = Value;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T: Into<Value>> From<T> for Literal {
    fn from(value: T) -> Self {
        Self::new(value.into())
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
