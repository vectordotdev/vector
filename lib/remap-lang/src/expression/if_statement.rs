use super::Error as E;
use crate::{state, value, Expr, Expression, Object, Result, TypeDef, Value};

#[derive(thiserror::Error, Clone, Debug, PartialEq)]
pub enum Error {
    #[error("invalid value kind")]
    Value(#[from] value::Error),
}

#[derive(Debug, Clone)]
pub struct IfStatement {
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
    fn execute(&self, state: &mut state::Program, object: &mut dyn Object) -> Result<Value> {
        match self.conditional.execute(state, object)? {
            Value::Boolean(true) => self.true_expression.execute(state, object),
            Value::Boolean(false) => self.false_expression.execute(state, object),
            v => Err(E::from(Error::from(value::Error::Expected(
                value::Kind::Boolean,
                v.kind(),
            )))
            .into()),
        }
    }

    fn type_def(&self, state: &state::Compiler) -> TypeDef {
        self.conditional
            .type_def(state)
            .fallible_unless(value::Kind::Boolean)
            .merge(self.true_expression.type_def(state))
            .merge(self.false_expression.type_def(state))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        expression::{Literal, Noop},
        test_type_def,
        value::Kind,
    };

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
                kind: Kind::Boolean,
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
                kind: Kind::all(),
            },
        }
    ];
}
