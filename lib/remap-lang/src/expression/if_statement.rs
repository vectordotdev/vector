use super::Error as E;
use crate::{state, value, Expr, Expression, Object, Result, TypeDef, Value};

#[derive(thiserror::Error, Clone, Debug, PartialEq)]
pub enum Error {
    #[error("conditional error")]
    Conditional(#[from] value::Error),
}

#[derive(Debug, Clone, PartialEq)]
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
        state: &state::Compiler,
    ) -> Result<Self> {
        let type_def = conditional.type_def(state);
        if !type_def.kind.is_boolean() {
            return Err(E::from(Error::Conditional(value::Error::Expected(
                value::Kind::Boolean,
                type_def.kind,
            )))
            .into());
        }

        Ok(Self {
            conditional,
            true_expression,
            false_expression,
        })
    }
}

impl Expression for IfStatement {
    fn execute(&self, state: &mut state::Program, object: &mut dyn Object) -> Result<Value> {
        let condition = self.conditional.execute(state, object)?.unwrap_boolean();

        match condition {
            true => self.true_expression.execute(state, object),
            false => self.false_expression.execute(state, object),
        }
    }

    fn type_def(&self, state: &state::Compiler) -> TypeDef {
        self.true_expression
            .type_def(state)
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
            expr: |state: &mut state::Compiler| {
                let conditional = Box::new(Literal::from(true).into());
                let true_expression = Box::new(Literal::from(true).into());
                let false_expression = Box::new(Literal::from(true).into());

                IfStatement::new(conditional, true_expression, false_expression, &state).unwrap()
            },
            def: TypeDef {
                kind: Kind::Boolean,
                ..Default::default()
            },
        }

        optional_null {
            expr: |state: &mut state::Compiler| {
                let conditional = Box::new(Literal::from(true).into());
                let true_expression = Box::new(Literal::from(true).into());
                let false_expression = Box::new(Noop.into());

                IfStatement::new(conditional, true_expression, false_expression, &state).unwrap()
            },
            def: TypeDef {
                kind: Kind::Boolean | Kind::Null,
                ..Default::default()
            },
        }
    ];
}
