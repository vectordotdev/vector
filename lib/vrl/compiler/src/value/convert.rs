use crate::{
    expression::{Container, Expr, Variant},
    Expression, Value,
};

/// Convert a given [`Value`] into a [`Expression`] trait object.
pub fn value_into_expression(value: Value) -> Box<dyn Expression> {
    Box::new(Expr::from(value))
}

/// Converts from an `Expr` into a `Value`. This is only possible if the expression represents
/// static values - `Literal`s and `Container`s containing `Literal`s.
/// The error returns the expression back so it can be used in the error report.
impl TryFrom<Expr> for Value {
    type Error = Expr;

    fn try_from(expr: Expr) -> Result<Self, Self::Error> {
        match expr {
            Expr::Literal(literal) => Ok(literal.to_value()),
            Expr::Container(Container {
                variant: Variant::Object(object),
            }) => Ok(Value::Object(
                object
                    .iter()
                    .map(|(key, value)| Ok((key.clone(), value.clone().try_into()?)))
                    .collect::<Result<_, Self::Error>>()?,
            )),
            Expr::Container(Container {
                variant: Variant::Array(array),
            }) => Ok(Value::Array(
                array
                    .iter()
                    .map(|value| value.clone().try_into())
                    .collect::<Result<_, _>>()?,
            )),
            expr => Err(expr),
        }
    }
}
