use crate::{state, value, Expr, Expression, Object, Result, TypeDef, Value};

#[derive(Debug, Clone, PartialEq)]
pub struct Not {
    expression: Box<Expr>,
}

impl Not {
    pub fn new(expression: Box<Expr>) -> Self {
        Self { expression }
    }
}

impl Expression for Not {
    fn execute(&self, state: &mut state::Program, object: &mut dyn Object) -> Result<Value> {
        let boolean = self.expression.execute(state, object)?.try_boolean()?;

        Ok((!boolean).into())
    }

    fn type_def(&self, state: &state::Compiler) -> TypeDef {
        self.expression
            .type_def(state)
            .fallible_unless(value::Kind::Boolean)
            .with_constraint(value::Kind::Boolean)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{expression::*, test_type_def, value::Kind};
    use std::collections::BTreeMap;

    #[test]
    fn not() {
        let cases = vec![
            (
                Ok(false.into()),
                Not::new(Box::new(Literal::from(true).into())),
            ),
            (
                Ok(true.into()),
                Not::new(Box::new(Literal::from(false).into())),
            ),
            (
                Err("value error".to_string()),
                Not::new(Box::new(Literal::from("not a bool").into())),
            ),
        ];

        let mut state = state::Program::default();
        let mut object: Value = BTreeMap::default().into();

        for (exp, func) in cases {
            let got = func
                .execute(&mut state, &mut object)
                .map_err(|e| e.to_string());

            assert_eq!(got, exp);
        }
    }

    test_type_def![boolean {
        expr: |_| Not::new(Box::new(Noop.into())),
        def: TypeDef {
            fallible: true,
            kind: Kind::Boolean,
            ..Default::default()
        },
    }];
}
