use super::Kind;
use crate::ExpressionError;
use diagnostic::DiagnosticError;

#[derive(thiserror::Error, Debug, PartialEq)]
pub enum Error {
    #[error(
        r#"expected {}, got "{got}""#,
        if .expected.is_many() {
            format!("{}", .expected)
        } else {
            format!(r#""{}""#, .expected)
        }
    )]
    Expected { got: Kind, expected: Kind },

    #[error(r#"unable to coerce "{0}" into "{1}""#)]
    Coerce(Kind, Kind),

    #[error("unable to calculate remainder of value type {0} and {1}")]
    Rem(Kind, Kind),

    #[error("unable to multiply value type {0} with {1}")]
    Mul(Kind, Kind),

    #[error("unable to divide value type {0} by {1}")]
    Div(Kind, Kind),

    #[error("unable to divide by zero")]
    DivideByZero,

    #[error("float cannot be NaN")]
    NanFloat,

    #[error("unable to add value type {1} to {0}")]
    Add(Kind, Kind),

    #[error("unable to subtract value type {1} from {0}")]
    Sub(Kind, Kind),

    #[error("unable to OR value types")]
    Or(#[from] ExpressionError),

    #[error("unable to AND value type {0} with {1}")]
    And(Kind, Kind),

    #[error("unable to compare {0} > {1}")]
    Gt(Kind, Kind),

    #[error("unable to compare {0} >= {1}")]
    Ge(Kind, Kind),

    #[error("unable to compare {0} < {1}")]
    Lt(Kind, Kind),

    #[error("unable to compare {0} <= {1}")]
    Le(Kind, Kind),
}

impl DiagnosticError for Error {
    fn code(&self) -> usize {
        use Error::*;

        match self {
            Expected { .. } => 300,
            Coerce(..) => 301,
            Rem(..) => 302,
            Mul(..) => 303,
            Div(..) => 304,
            DivideByZero => 305,
            NanFloat => 306,
            Add(..) => 307,
            Sub(..) => 308,
            Or(..) => 309,
            And(..) => 310,
            Gt(..) => 311,
            Ge(..) => 312,
            Lt(..) => 313,
            Le(..) => 314,
        }
    }
}

impl From<Error> for ExpressionError {
    fn from(err: Error) -> Self {
        ExpressionError {
            message: err.message(),
            ..Default::default()
        }
    }
}
