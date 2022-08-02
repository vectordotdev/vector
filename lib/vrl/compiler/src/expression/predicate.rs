use std::fmt;

use diagnostic::{DiagnosticMessage, Label, Note, Urls};
use value::Value;

use crate::{
    expression::{Expr, Resolved},
    parser::Node,
    state::{ExternalEnv, LocalEnv},
    value::Kind,
    BatchContext, Context, Expression, Span, TypeDef,
};

#[derive(Clone, PartialEq)]
pub struct Predicate {
    inner: Vec<Expr>,
    selection_vector_this: Vec<usize>,
    selection_vector_other: Vec<usize>,
}

impl Predicate {
    pub(crate) fn new(
        node: Node<Vec<Expr>>,
        state: (&LocalEnv, &ExternalEnv),
        fallible_predicate: Option<&dyn DiagnosticMessage>,
    ) -> Result<Predicate, Error> {
        let (span, exprs) = node.take();
        let type_def = exprs
            .last()
            .map_or_else(TypeDef::null, |expr| expr.type_def(state));

        if let Some(error) = fallible_predicate {
            return Err(Error::Fallible {
                code: error.code(),
                labels: error.labels(),
                notes: error.notes(),
            });
        }

        if !type_def.is_boolean() {
            return Err(Error::NonBoolean {
                kind: type_def.into(),
                span,
            });
        }

        Ok(Self::new_unchecked(exprs))
    }

    #[must_use]
    pub fn new_unchecked(inner: Vec<Expr>) -> Self {
        Self {
            inner,
            selection_vector_this: vec![],
            selection_vector_other: vec![],
        }
    }
}

impl Expression for Predicate {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        self.inner
            .iter()
            .map(|expr| expr.resolve(ctx))
            .collect::<std::result::Result<Vec<_>, _>>()
            .map(|mut v| v.pop().unwrap_or(Value::Boolean(false)))
    }

    fn resolve_batch(&mut self, ctx: &mut BatchContext, selection_vector: &[usize]) {
        if self.inner.len() == 1 {
            self.inner[0].resolve_batch(ctx, selection_vector);
        } else {
            self.selection_vector_this.resize(selection_vector.len(), 0);
            self.selection_vector_this.copy_from_slice(selection_vector);

            for block in &mut self.inner {
                block.resolve_batch(ctx, &self.selection_vector_this);
                self.selection_vector_other.truncate(0);

                for index in selection_vector {
                    let index = *index;
                    if !matches!(&ctx.resolved_values[index], Err(error) if error.is_abort()) {
                        self.selection_vector_other.push(index);
                    }
                }

                std::mem::swap(
                    &mut self.selection_vector_this,
                    &mut self.selection_vector_other,
                );
            }
        }
    }

    fn type_def(&self, state: (&LocalEnv, &ExternalEnv)) -> TypeDef {
        let mut type_defs = self
            .inner
            .iter()
            .map(|expr| expr.type_def(state))
            .collect::<Vec<_>>();

        // If any of the stored expressions is fallible, the entire predicate is
        // fallible.
        let fallible = type_defs.iter().any(TypeDef::is_fallible);

        let abortable = type_defs.iter().any(TypeDef::is_abortable);

        // The last expression determines the resulting value of the predicate.
        let type_def = type_defs.pop().unwrap_or_else(TypeDef::boolean);

        type_def
            .with_fallibility(fallible)
            .with_abortability(abortable)
    }

    #[cfg(feature = "llvm")]
    fn emit_llvm<'ctx>(
        &self,
        state: (&LocalEnv, &ExternalEnv),
        ctx: &mut crate::llvm::Context<'ctx>,
    ) -> std::result::Result<(), String> {
        let predicate_begin_block = ctx.append_basic_block("predicate_begin");
        let predicate_end_block = ctx.append_basic_block("predicate_end");

        ctx.build_unconditional_branch(predicate_begin_block);
        ctx.position_at_end(predicate_begin_block);

        for inner in &self.inner {
            ctx.emit_llvm_abortable(inner, state, ctx.result_ref(), predicate_end_block, vec![])?;
        }
        ctx.build_unconditional_branch(predicate_end_block);

        ctx.position_at_end(predicate_end_block);

        Ok(())
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
                Label::context(format!("instead it resolves to {}", kind), span),
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
