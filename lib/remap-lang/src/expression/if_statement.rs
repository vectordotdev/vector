use crate::{state, value, Expr, Expression, Object, TypeDef, Value};

#[derive(thiserror::Error, Clone, Debug, PartialEq)]
pub enum Error {
    #[error("conditional error")]
    Conditional(#[from] value::Error),
}

/// Wrapper type for an if condition.
///
/// The initializer of this type errors if the condition doesn't resolve to a
/// boolean.
pub struct IfCondition(Box<Expr>);

impl IfCondition {
    pub fn new(expression: Box<Expr>, state: &state::Compiler) -> Result<Self, Error> {
        let kind = expression.type_def(state).kind;
        if !kind.is_boolean() {
            return Err(Error::Conditional(value::Error::Expected(
                value::Kind::Boolean,
                kind,
            )));
        }

        Ok(Self(expression))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct IfStatement {
    conditional: Box<Expr>,
    true_expression: Box<Expr>,
    false_expression: Box<Expr>,
}

impl IfStatement {
    pub fn new(
        conditional: IfCondition,
        true_expression: Box<Expr>,
        false_expression: Box<Expr>,
    ) -> Self {
        Self {
            conditional: conditional.0,
            true_expression,
            false_expression,
        }
    }
}

impl Expression for IfStatement {
    fn execute(&self, state: &mut state::Program, object: &mut dyn Object) -> crate::Result<Value> {
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
        lit, test_type_def,
        value::Kind,
    };

    test_type_def![
        concrete_type_def {
            expr: |state: &mut state::Compiler| {
                let conditional = IfCondition(lit!(true).boxed());
                let true_expression = Exor::from(lit!(true)).boxed();
                let false_expression = Expr::from(lit!(true)).boxed();

                IfStatement::new(conditional, true_expression, false_expression)
            },
            def: TypeDef {
                kind: Kind::Boolean,
                ..Default::default()
            },
        }

        optional_null {
            expr: |state: &mut state::Compiler| {
                let conditional = IfCondition(lit!(true).boxed());
                let true_expression = Exor::from(lit!(true)).boxed();
                let false_expression = Box::new(Noop.into());

                IfStatement::new(conditional, true_expression, false_expression)
            },
            def: TypeDef {
                kind: Kind::Boolean | Kind::Null,
                ..Default::default()
            },
        }
    ];
}
