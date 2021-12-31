use diagnostic::DiagnosticError;

use super::Kind;
use crate::ExpressionError;

#[derive(thiserror::Error, Debug, PartialEq)]
pub enum Error {
    #[error(
        r#"expected {}, got {got}"#,
        .expected
    )]
    Expected { got: Kind, expected: Kind },

    #[error(r#"can't coerce {0} into {1}"#)]
    Coerce(Kind, Kind),

    #[error("can't calculate remainder of type {0} and {1}")]
    Rem(Kind, Kind),

    #[error("can't multiply type {0} by {1}")]
    Mul(Kind, Kind),

    #[error("can't divide type {0} by {1}")]
    Div(Kind, Kind),

    #[error("can't divide by zero")]
    DivideByZero,

    #[error("floats can't be NaN")]
    NanFloat,

    #[error("can't add type {1} to {0}")]
    Add(Kind, Kind),

    #[error("can't subtract type {1} from {0}")]
    Sub(Kind, Kind),

    #[error("can't apply an OR to these types - {0}")]
    Or(#[from] ExpressionError),

    #[error("can't apply an AND to types {0} and {1}")]
    And(Kind, Kind),

    #[error("can't compare {0} > {1}")]
    Gt(Kind, Kind),

    #[error("can't compare {0} >= {1}")]
    Ge(Kind, Kind),

    #[error("can't compare {0} < {1}")]
    Lt(Kind, Kind),

    #[error("can't compare {0} <= {1}")]
    Le(Kind, Kind),

    #[error("can't merge type {1} into {0}")]
    Merge(Kind, Kind),
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
            Merge(..) => 315,
        }
    }
}

impl From<Error> for ExpressionError {
    fn from(err: Error) -> Self {
        Self::Error {
            message: err.message(),
            labels: vec![],
            notes: vec![],
        }
    }
}
