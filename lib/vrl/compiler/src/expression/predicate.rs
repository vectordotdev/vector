use std::fmt;

use diagnostic::{DiagnosticError, Label, Note, Urls};

use crate::{
    expression::{Block, Expr, Resolved},
    parser::Node,
    value::Kind,
    Context, Expression, Span, State, TypeDef, Value,
};

pub type Result = std::result::Result<Predicate, Error>;

#[derive(Clone, PartialEq)]
pub struct Predicate {
    inner: Vec<Expr>,
}

impl Predicate {
    pub fn new(node: Node<Block>, state: &State) -> Result {
        let (span, block) = node.take();
        let type_def = block.type_def(state);

        if !type_def.is_boolean() {
            return Err(Error {
                variant: ErrorVariant::NonBoolean(type_def.kind()),
                span,
            });
        }

        Ok(Self {
            inner: block.into_inner(),
        })
    }

    pub fn new_unchecked(inner: Vec<Expr>) -> Self {
        Self { inner }
    }
}

impl Expression for Predicate {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        self.inner
            .iter()
            .map(|expr| expr.resolve(ctx))
            .collect::<std::result::Result<Vec<_>, _>>()
            .map(|mut v| v.pop().unwrap_or(Value::Null))
    }

    fn type_def(&self, state: &State) -> TypeDef {
        let mut type_defs = self
            .inner
            .iter()
            .map(|expr| expr.type_def(state))
            .collect::<Vec<_>>();

        // If any of the stored expressions is fallible, the entire predicate is
        // fallible.
        let fallible = type_defs.iter().any(TypeDef::is_fallible);

        // The last expression determines the resulting value of the predicate.
        let type_def = type_defs.pop().unwrap_or_else(|| TypeDef::new().null());

        type_def.with_fallibility(fallible)
    }
}

impl fmt::Display for Predicate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.inner.len() > 1 {
            f.write_str("(")?;
        }

        let mut iter = self.inner.iter().peekable();
        while let Some(expr) = iter.next() {
            expr.fmt(f)?;

            if iter.peek().is_some() {
                f.write_str("; ")?;
            }
        }

        if self.inner.len() > 1 {
            f.write_str("(")?;
        }

        Ok(())
    }
}

impl fmt::Debug for Predicate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Predicate(")?;

        let mut iter = self.inner.iter().peekable();
        while let Some(expr) = iter.next() {
            expr.fmt(f)?;

            if iter.peek().is_some() {
                f.write_str("; ")?;
            }
        }

        f.write_str(")")
    }
}

// -----------------------------------------------------------------------------

#[derive(Debug)]
pub struct Error {
    pub(crate) variant: ErrorVariant,

    span: Span,
}

#[derive(thiserror::Error, Debug)]
pub enum ErrorVariant {
    #[error("non-boolean predicate")]
    NonBoolean(Kind),
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

impl DiagnosticError for Error {
    fn code(&self) -> usize {
        use ErrorVariant::*;

        match &self.variant {
            NonBoolean(..) => 102,
        }
    }

    fn labels(&self) -> Vec<Label> {
        use ErrorVariant::*;

        match &self.variant {
            NonBoolean(kind) => vec![
                Label::primary("this predicate must resolve to a boolean", self.span),
                Label::context(format!("instead it resolves to {}", kind), self.span),
            ],
        }
    }

    fn notes(&self) -> Vec<Note> {
        use ErrorVariant::*;

        match &self.variant {
            NonBoolean(..) => vec![
                Note::CoerceValue,
                Note::SeeDocs(
                    "if expressions".to_owned(),
                    Urls::expression_docs_url("#if"),
                ),
            ],
        }
    }
}
