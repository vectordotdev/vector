use std::fmt;

use diagnostic::{DiagnosticMessage, Label, Note, Urls};

use crate::expression::Block;
use crate::{
    expression::{Expr, Resolved},
    parser::Node,
    state::{TypeInfo, TypeState},
    value::Kind,
    Context, Expression, Span,
};

pub(crate) type Result = std::result::Result<Predicate, Error>;

#[derive(Clone, PartialEq)]
pub struct Predicate {
    inner: Block,
}

impl Predicate {
    pub(crate) fn new(
        node: Node<Vec<Expr>>,
        state: &TypeState,
        fallible_predicate: Option<&dyn DiagnosticMessage>,
    ) -> Result {
        let (span, exprs) = node.take();

        if let Some(error) = fallible_predicate {
            return Err(Error::Fallible {
                code: error.code(),
                labels: error.labels(),
                notes: error.notes(),
            });
        }

        let block = Block::new_inline(exprs);
        let type_def = block.type_info(state).result;
        if !type_def.is_boolean() {
            return Err(Error::NonBoolean {
                kind: type_def.into(),
                span,
            });
        }

        Ok(Self { inner: block })
    }

    #[must_use]
    pub fn new_unchecked(inner: Vec<Expr>) -> Self {
        Self {
            inner: Block::new_inline(inner),
        }
    }
}

impl Expression for Predicate {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        self.inner.resolve(ctx)
    }

    fn type_info(&self, state: &TypeState) -> TypeInfo {
        self.inner.type_info(state)
    }
}

impl fmt::Display for Predicate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.inner.exprs().len() > 1 {
            f.write_str("(")?;
        }

        let mut iter = self.inner.exprs().iter().peekable();
        while let Some(expr) = iter.next() {
            expr.fmt(f)?;

            if iter.peek().is_some() {
                f.write_str("; ")?;
            }
        }

        if self.inner.exprs().len() > 1 {
            f.write_str("(")?;
        }

        Ok(())
    }
}

impl fmt::Debug for Predicate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Predicate(")?;

        let mut iter = self.inner.exprs().iter().peekable();
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

#[derive(thiserror::Error, Debug)]
pub(crate) enum Error {
    #[error("non-boolean predicate")]
    NonBoolean { kind: Kind, span: Span },

    #[error("fallible predicate")]
    Fallible {
        code: usize,
        labels: Vec<Label>,
        notes: Vec<Note>,
    },
}

impl DiagnosticMessage for Error {
    fn code(&self) -> usize {
        use Error::{Fallible, NonBoolean};

        match self {
            NonBoolean { .. } => 102,
            Fallible { code, .. } => *code,
        }
    }

    fn labels(&self) -> Vec<Label> {
        use Error::{Fallible, NonBoolean};

        match self {
            NonBoolean { kind, span } => vec![
                Label::primary("this predicate must resolve to a boolean", span),
                Label::context(format!("instead it resolves to {kind}"), span),
            ],
            Fallible { labels, .. } => labels.clone(),
        }
    }

    fn notes(&self) -> Vec<Note> {
        use Error::{Fallible, NonBoolean};

        match self {
            NonBoolean { .. } => vec![
                Note::CoerceValue,
                Note::SeeDocs(
                    "if expressions".to_owned(),
                    Urls::expression_docs_url("#if"),
                ),
            ],
            Fallible { notes, .. } => notes.clone(),
        }
    }
}
