use std::{borrow::Cow, convert::TryFrom, fmt};

use bytes::Bytes;
use chrono::{DateTime, Utc};
use diagnostic::{DiagnosticMessage, Label, Note, Urls};
use ordered_float::NotNan;
use regex::Regex;
use value::{Value, ValueRegex};

use crate::{
    expression::Resolved,
    state::{ExternalEnv, LocalEnv},
    vm::OpCode,
    Context, Expression, Span, TypeDef,
};

#[derive(Debug, Clone, PartialEq)]
pub enum Literal {
    String(Value, Bytes),
    Integer(Value, i64),
    Float(Value, NotNan<f64>),
    Boolean(Value, bool),
    Regex(Value, ValueRegex),
    Timestamp(Value, DateTime<Utc>),
    Null,
}

impl Literal {
    /// Get a `Value` type stored in the literal.
    ///
    /// This differs from `Expression::as_value` insofar as this *always*
    /// returns a `Value`, whereas `as_value` returns `Option<Value>` which, in
    /// the case of `Literal` means it always returns `Some(Value)`, requiring
    /// an extra `unwrap()`.
    pub fn to_value(&self) -> Value {
        use Literal::*;

        match self {
            String(v, _)
            | Integer(v, _)
            | Float(v, _)
            | Boolean(v, _)
            | Regex(v, _)
            | Timestamp(v, _) => v.clone(),
            Null => Value::Null,
        }
    }
}

impl Expression for Literal {
    fn resolve<'value, 'ctx: 'value, 'rt: 'ctx>(
        &'rt self,
        _: &'ctx mut Context,
    ) -> Resolved<'value> {
        use Literal::*;

        Ok(match self {
            String(v, _)
            | Integer(v, _)
            | Float(v, _)
            | Boolean(v, _)
            | Regex(v, _)
            | Timestamp(v, _) => Cow::Borrowed(v),
            Null => Cow::Owned(Value::Null),
        })
    }

    fn as_value(&self) -> Option<Value> {
        Some(self.to_value())
    }

    fn type_def(&self, _: (&LocalEnv, &ExternalEnv)) -> TypeDef {
        use Literal::*;

        let type_def = match self {
            String(_, _) => TypeDef::bytes(),
            Integer(_, _) => TypeDef::integer(),
            Float(_, _) => TypeDef::float(),
            Boolean(_, _) => TypeDef::boolean(),
            Regex(_, _) => TypeDef::regex(),
            Timestamp(_, _) => TypeDef::timestamp(),
            Null => TypeDef::null(),
        };

        type_def.infallible()
    }

    fn compile_to_vm(
        &self,
        vm: &mut crate::vm::Vm,
        _state: (&mut LocalEnv, &mut ExternalEnv),
    ) -> Result<(), String> {
        // Add the literal as a constant.
        let constant = vm.add_constant(self.to_value());
        vm.write_opcode(OpCode::Constant);
        vm.write_primitive(constant);
        Ok(())
    }
}

impl fmt::Display for Literal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use Literal::*;

        match self {
            String(v, _)
            | Integer(v, _)
            | Float(v, _)
            | Boolean(v, _)
            | Regex(v, _)
            | Timestamp(v, _) => v.fmt(f),
            Null => f.write_str("null"),
        }
    }
}

// Literal::String -------------------------------------------------------------

impl From<Bytes> for Literal {
    fn from(v: Bytes) -> Self {
        Literal::String(v.clone().into(), v)
    }
}

impl From<Cow<'_, str>> for Literal {
    fn from(v: Cow<'_, str>) -> Self {
        v.as_ref().into()
    }
}

impl From<Vec<u8>> for Literal {
    fn from(v: Vec<u8>) -> Self {
        v.as_slice().into()
    }
}

impl From<&[u8]> for Literal {
    fn from(v: &[u8]) -> Self {
        Bytes::copy_from_slice(v).into()
    }
}

impl From<String> for Literal {
    fn from(v: String) -> Self {
        Bytes::from(v).into()
    }
}

impl From<&str> for Literal {
    fn from(v: &str) -> Self {
        v.as_bytes().into()
    }
}

// Literal::Integer ------------------------------------------------------------

impl From<i64> for Literal {
    fn from(v: i64) -> Self {
        Literal::Integer(v.into(), v)
    }
}

impl From<i8> for Literal {
    fn from(v: i8) -> Self {
        (v as i64).into()
    }
}

impl From<i16> for Literal {
    fn from(v: i16) -> Self {
        (v as i64).into()
    }
}

impl From<i32> for Literal {
    fn from(v: i32) -> Self {
        (v as i64).into()
    }
}

impl From<u16> for Literal {
    fn from(v: u16) -> Self {
        (v as i64).into()
    }
}

impl From<u32> for Literal {
    fn from(v: u32) -> Self {
        (v as i64).into()
    }
}

impl From<u64> for Literal {
    fn from(v: u64) -> Self {
        (v as i64).into()
    }
}

impl From<usize> for Literal {
    fn from(v: usize) -> Self {
        (v as i64).into()
    }
}

// Literal::Float --------------------------------------------------------------

impl From<NotNan<f64>> for Literal {
    fn from(v: NotNan<f64>) -> Self {
        Literal::Float(v.into(), v)
    }
}

impl TryFrom<f64> for Literal {
    type Error = Error;

    fn try_from(v: f64) -> Result<Self, Self::Error> {
        NotNan::new(v)
            .map_err(|_| Error {
                span: Span::default(),
                variant: ErrorVariant::NanFloat,
            })
            .map(Into::into)
    }
}

// Literal::Boolean ------------------------------------------------------------

impl From<bool> for Literal {
    fn from(v: bool) -> Self {
        Literal::Boolean(v.into(), v)
    }
}

// Literal::Regex --------------------------------------------------------------

impl From<Regex> for Literal {
    fn from(regex: Regex) -> Self {
        ValueRegex::new(regex).into()
    }
}

impl From<ValueRegex> for Literal {
    fn from(regex: ValueRegex) -> Self {
        Literal::Regex(regex.clone().into(), regex)
    }
}

// Literal::Null ---------------------------------------------------------------

impl From<()> for Literal {
    fn from(_: ()) -> Self {
        Literal::Null
    }
}

impl<T: Into<Literal>> From<Option<T>> for Literal {
    fn from(literal: Option<T>) -> Self {
        match literal {
            None => Literal::Null,
            Some(v) => v.into(),
        }
    }
}

// Literal::Regex --------------------------------------------------------------

impl From<DateTime<Utc>> for Literal {
    fn from(dt: DateTime<Utc>) -> Self {
        Literal::Timestamp(dt.into(), dt)
    }
}

// -----------------------------------------------------------------------------

#[derive(Debug)]
pub struct Error {
    pub(crate) variant: ErrorVariant,
    span: Span,
}

#[derive(thiserror::Error, Debug)]
pub(crate) enum ErrorVariant {
    #[error("invalid regular expression")]
    InvalidRegex(#[from] regex::Error),

    #[error("invalid timestamp")]
    InvalidTimestamp(#[from] chrono::ParseError),

    #[error("float literal can't be NaN")]
    NanFloat,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:#}", self.variant)
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.variant)
    }
}

impl DiagnosticMessage for Error {
    fn code(&self) -> usize {
        use ErrorVariant::*;

        match &self.variant {
            InvalidRegex(..) => 101,
            InvalidTimestamp(..) => 601,
            NanFloat => 602,
        }
    }

    fn labels(&self) -> Vec<Label> {
        use ErrorVariant::*;

        match &self.variant {
            InvalidRegex(err) => {
                let error = err
                    .to_string()
                    .lines()
                    .filter_map(|line| {
                        if line.trim() == "^" || line == "regex parse error:" {
                            return None;
                        }

                        Some(line.trim_start_matches("error: ").trim())
                    })
                    .rev()
                    .collect::<Vec<_>>()
                    .join(": ");

                vec![Label::primary(
                    format!("regex parse error: {}", error),
                    self.span,
                )]
            }
            InvalidTimestamp(err) => vec![Label::primary(
                format!("invalid timestamp format: {}", err),
                self.span,
            )],

            NanFloat => vec![],
        }
    }

    fn notes(&self) -> Vec<Note> {
        use ErrorVariant::*;

        match &self.variant {
            InvalidRegex(_) => vec![Note::SeeDocs(
                "regular expressions".to_owned(),
                Urls::expression_docs_url("#regular-expression"),
            )],
            InvalidTimestamp(_) => vec![Note::SeeDocs(
                "timestamps".to_owned(),
                Urls::expression_docs_url("#timestamp"),
            )],
            NanFloat => vec![Note::SeeDocs(
                "floats".to_owned(),
                Urls::expression_docs_url("#float"),
            )],
        }
    }
}

impl From<(Span, regex::Error)> for Error {
    fn from((span, err): (Span, regex::Error)) -> Self {
        Self {
            variant: err.into(),
            span,
        }
    }
}

impl From<(Span, chrono::ParseError)> for Error {
    fn from((span, err): (Span, chrono::ParseError)) -> Self {
        Self {
            variant: err.into(),
            span,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{expr, test_type_def, TypeDef};

    test_type_def![
        bytes {
            expr: |_| expr!("foo"),
            want: TypeDef::bytes(),
        }

        integer {
            expr: |_| expr!(12),
            want: TypeDef::integer(),
        }
    ];
}
