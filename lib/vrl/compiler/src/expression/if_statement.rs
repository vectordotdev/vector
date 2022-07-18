use std::fmt;

use value::Value;

use crate::{
    expression::{Block, Predicate, Resolved},
    state::{ExternalEnv, LocalEnv},
    value::VrlValueConvert,
    BatchContext, Context, Expression, TypeDef,
};

#[derive(Debug, Clone, PartialEq)]
pub struct IfStatement {
    predicate: Predicate,
    consequent: Block,
    alternative: Option<Block>,
    selection_vector_ok: Vec<usize>,
    selection_vector_if: Vec<usize>,
    selection_vector_else: Vec<usize>,
}

impl IfStatement {
    #[must_use]
    pub fn new(predicate: Predicate, consequent: Block, alternative: Option<Block>) -> Self {
        Self {
            predicate,
            consequent,
            alternative,
            selection_vector_ok: vec![],
            selection_vector_if: vec![],
            selection_vector_else: vec![],
        }
    }
}

impl Expression for IfStatement {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let predicate = self
            .predicate
            .resolve(ctx)?
            .try_boolean()
            .expect("predicate must be boolean");

        match predicate {
            true => self.consequent.resolve(ctx),
            false => self
                .alternative
                .as_ref()
                .map_or(Ok(Value::Null), |block| block.resolve(ctx)),
        }
    }

    fn resolve_batch(&mut self, ctx: &mut BatchContext, selection_vector: &[usize]) {
        self.predicate.resolve_batch(ctx, selection_vector);

        self.selection_vector_ok.truncate(0);

        for index in selection_vector {
            let index = *index;
            if ctx.resolved_values[index].is_ok() {
                self.selection_vector_ok.push(index);
            }
        }

        self.selection_vector_if.truncate(0);
        self.selection_vector_else.truncate(0);

        for index in &self.selection_vector_ok {
            let index = *index;
            let predicate = match ctx.resolved_values.get(index) {
                Some(Ok(Value::Boolean(predicate))) => *predicate,
                _ => unreachable!("predicate has been checked for error and must be boolean"),
            };

            if predicate {
                self.selection_vector_if.push(index);
            } else {
                self.selection_vector_else.push(index);
            }
        }

        self.consequent
            .resolve_batch(ctx, &self.selection_vector_if);
        if let Some(alternative) = &mut self.alternative {
            alternative.resolve_batch(ctx, &self.selection_vector_else);
        } else {
            for index in &self.selection_vector_else {
                ctx.resolved_values[*index] = Ok(Value::Null);
            }
        }
    }

    fn type_def(&self, state: (&LocalEnv, &ExternalEnv)) -> TypeDef {
        let type_def = self.consequent.type_def(state);

        match &self.alternative {
            None => type_def.add_null(),
            Some(alternative) => type_def.merge_deep(alternative.type_def(state)),
        }
    }
}

impl fmt::Display for IfStatement {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("if ")?;
        self.predicate.fmt(f)?;
        f.write_str(" ")?;
        self.consequent.fmt(f)?;

        if let Some(alt) = &self.alternative {
            f.write_str(" else")?;
            alt.fmt(f)?;
        }

        Ok(())
    }
}
