use crate::{CompilerState, Expression, Object, Result, State, TypeCheck, Value, ValueConstraint};

#[derive(Debug, Clone)]
pub struct Literal(Value);

impl<T: Into<Value>> From<T> for Literal {
    fn from(value: T) -> Self {
        Self(value.into())
    }
}

impl Expression for Literal {
    fn execute(&self, _: &mut State, _: &mut dyn Object) -> Result<Option<Value>> {
        Ok(Some(self.0.clone()))
    }

    fn type_check(&self, _: &CompilerState) -> TypeCheck {
        TypeCheck {
            constraint: ValueConstraint::Exact(self.0.kind()),
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{test_type_check, Literal, ValueConstraint::*, ValueKind::*};
    use std::collections::BTreeMap;

    test_type_check![
        boolean {
            expr: |_| Literal::from(true),
            def: TypeCheck { constraint: Exact(Boolean), ..Default::default() },
        }

        string {
            expr: |_| Literal::from("foo"),
            def: TypeCheck { constraint: Exact(String), ..Default::default() },
        }

        integer {
            expr: |_| Literal::from(123),
            def: TypeCheck { constraint: Exact(Integer), ..Default::default() },
        }

        float {
            expr: |_| Literal::from(123.456),
            def: TypeCheck { constraint: Exact(Float), ..Default::default() },
        }

        array {
            expr: |_| Literal::from(vec!["foo"]),
            def: TypeCheck { constraint: Exact(Array), ..Default::default() },
        }

        map {
            expr: |_| Literal::from(BTreeMap::default()),
            def: TypeCheck { constraint: Exact(Map), ..Default::default() },
        }

        timestamp {
            expr: |_| Literal::from(chrono::Utc::now()),
            def: TypeCheck { constraint: Exact(Timestamp), ..Default::default() },
        }

        null {
            expr: |_| Literal::from(()),
            def: TypeCheck { constraint: Exact(Null), ..Default::default() },
        }
    ];
}
