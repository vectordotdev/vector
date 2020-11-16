use crate::{state, value, Expression, Object, Result, TypeDef, Value};

#[derive(Debug, Clone)]
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
    fn execute(&self, _: &mut state::Program, _: &mut dyn Object) -> Result<Option<Value>> {
        Ok(Some(self.0.clone()))
    }

    fn type_def(&self, _: &state::Compiler) -> TypeDef {
        TypeDef {
            constraint: value::Constraint::Exact(self.0.kind()),
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{test_type_def, value::Constraint::*, value::Kind::*};
    use std::collections::BTreeMap;

    test_type_def![
        boolean {
            expr: |_| Literal::from(true),
            def: TypeDef { constraint: Exact(Boolean), ..Default::default() },
        }

        string {
            expr: |_| Literal::from("foo"),
            def: TypeDef { constraint: Exact(String), ..Default::default() },
        }

        integer {
            expr: |_| Literal::from(123),
            def: TypeDef { constraint: Exact(Integer), ..Default::default() },
        }

        float {
            expr: |_| Literal::from(123.456),
            def: TypeDef { constraint: Exact(Float), ..Default::default() },
        }

        array {
            expr: |_| Literal::from(vec!["foo"]),
            def: TypeDef { constraint: Exact(Array), ..Default::default() },
        }

        map {
            expr: |_| Literal::from(BTreeMap::default()),
            def: TypeDef { constraint: Exact(Map), ..Default::default() },
        }

        timestamp {
            expr: |_| Literal::from(chrono::Utc::now()),
            def: TypeDef { constraint: Exact(Timestamp), ..Default::default() },
        }

        null {
            expr: |_| Literal::from(()),
            def: TypeDef { constraint: Exact(Null), ..Default::default() },
        }
    ];
}
