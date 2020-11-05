use super::Error as E;
use crate::{value, Expr, Expression, Object, Result, State, Value, ValueKind};

#[derive(thiserror::Error, Debug, PartialEq)]
pub enum Error {
    #[error("invalid value kind")]
    Value(#[from] value::Error),
}

#[derive(Debug, Clone)]
pub(crate) struct Not {
    expression: Box<Expr>,
}

impl Not {
    pub fn new(expression: Box<Expr>) -> Self {
        Self { expression }
    }
}

impl Expression for Not {
    fn execute(&self, state: &mut State, object: &mut dyn Object) -> Result<Option<Value>> {
        self.expression.execute(state, object).and_then(|opt| {
            opt.map(|v| match v {
                Value::Boolean(b) => Ok(Value::Boolean(!b)),
                _ => Err(E::from(Error::from(value::Error::Expected(
                    ValueKind::Boolean,
                    v.kind(),
                )))
                .into()),
            })
            .transpose()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn not() {
        let cases = vec![
            (
                Err("path error".to_string()),
                Not::new(Box::new(crate::Path::from("foo").into())),
            ),
            (
                Ok(Some(false.into())),
                Not::new(Box::new(crate::Literal::from(true).into())),
            ),
            (
                Ok(Some(true.into())),
                Not::new(Box::new(crate::Literal::from(false).into())),
            ),
            (
                Err("not operation error".to_string()),
                Not::new(Box::new(crate::Literal::from("not a bool").into())),
            ),
        ];

        let mut state = crate::State::default();
        let mut object = std::collections::HashMap::default();

        for (exp, func) in cases {
            let got = func
                .execute(&mut state, &mut object)
                .map_err(|e| e.to_string());

            assert_eq!(got, exp);
        }
    }
}
