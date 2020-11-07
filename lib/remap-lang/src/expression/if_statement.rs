use super::Error as E;
use crate::{
    value, CompilerState, Expr, Expression, Object, Result, State, TypeDef, Value, ValueKind,
};

#[derive(thiserror::Error, Debug, PartialEq)]
pub enum Error {
    #[error("invalid value kind")]
    Value(#[from] value::Error),
}

#[derive(Debug, Clone)]
pub(crate) struct IfStatement {
    conditional: Box<Expr>,
    true_expression: Box<Expr>,
    false_expression: Box<Expr>,
}

impl IfStatement {
    pub fn new(
        conditional: Box<Expr>,
        true_expression: Box<Expr>,
        false_expression: Box<Expr>,
    ) -> Self {
        Self {
            conditional,
            true_expression,
            false_expression,
        }
    }
}

impl Expression for IfStatement {
    fn execute(&self, state: &mut State, object: &mut dyn Object) -> Result<Option<Value>> {
        match self.conditional.execute(state, object)? {
            Some(Value::Boolean(true)) => self.true_expression.execute(state, object),
            Some(Value::Boolean(false)) | None => self.false_expression.execute(state, object),
            Some(v) => Err(E::from(Error::from(value::Error::Expected(
                ValueKind::Boolean,
                v.kind(),
            )))
            .into()),
        }
    }

    fn type_def(&self, state: &CompilerState) -> TypeDef {
        let true_type_def = self.true_expression.type_def(state);
        let false_type_def = self.false_expression.type_def(state);

        true_type_def.merge(&false_type_def)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{test_type_def, Literal, Noop, ValueConstraint::*, ValueKind::*};

    test_type_def![
        concrete_type_def {
            expr: |_| {
                let conditional = Box::new(Literal::from(true).into());
                let true_expression = Box::new(Literal::from(true).into());
                let false_expression = Box::new(Literal::from(true).into());

                IfStatement::new(conditional, true_expression, false_expression)
            },
            def: TypeDef {
                fallible: false,
                optional: false,
                constraint: Exact(Boolean),
            },
        }

        optional_any {
            expr: |_| {
                let conditional = Box::new(Literal::from(true).into());
                let true_expression = Box::new(Literal::from(true).into());
                let false_expression = Box::new(Noop.into());

                IfStatement::new(conditional, true_expression, false_expression)
            },
            def: TypeDef {
                fallible: false,
                optional: true,
                constraint: Any,
            },
        }
    ];
}
