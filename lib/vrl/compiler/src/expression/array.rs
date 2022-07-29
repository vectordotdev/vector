use std::{collections::BTreeMap, fmt, ops::Deref};

use value::Value;

use crate::{
    expression::{Expr, Resolved},
    state::{ExternalEnv, LocalEnv},
    Context, Expression, TypeDef,
};

#[derive(Debug, Clone, PartialEq)]
pub struct Array {
    inner: Vec<Expr>,
}

impl Array {
    pub(crate) fn new(inner: Vec<Expr>) -> Self {
        Self { inner }
    }
}

impl Deref for Array {
    type Target = Vec<Expr>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl Expression for Array {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        self.inner
            .iter()
            .map(|expr| expr.resolve(ctx))
            .collect::<Result<Vec<_>, _>>()
            .map(Value::Array)
    }

    fn as_value(&self) -> Option<Value> {
        self.inner
            .iter()
            .map(Expr::as_value)
            .collect::<Option<Vec<_>>>()
            .map(Value::Array)
    }

    fn type_def(&self, state: (&LocalEnv, &ExternalEnv)) -> TypeDef {
        let type_defs = self
            .inner
            .iter()
            .map(|expr| expr.type_def(state))
            .collect::<Vec<_>>();

        // If any of the stored expressions is fallible, the entire array is
        // fallible.
        let fallible = type_defs.iter().any(TypeDef::is_fallible);

        let abortable = type_defs.iter().any(TypeDef::is_abortable);

        let collection = type_defs
            .into_iter()
            .enumerate()
            .map(|(index, type_def)| (index.into(), type_def.into()))
            .collect::<BTreeMap<_, _>>();

        TypeDef::array(collection)
            .with_fallibility(fallible)
            .with_abortability(abortable)
    }

    #[cfg(feature = "llvm")]
    fn emit_llvm<'ctx>(
        &self,
        state: (&mut LocalEnv, &mut ExternalEnv),
        ctx: &mut crate::llvm::Context<'ctx>,
    ) -> Result<(), String> {
        let begin_block = ctx.append_basic_block("array_begin");
        let end_block = ctx.append_basic_block("array_end");
        let insert_block = ctx.append_basic_block("array_insert");
        let set_result_block = ctx.append_basic_block("array_set_result");

        ctx.build_unconditional_branch(begin_block);
        ctx.position_at_end(begin_block);

        let vec_ref = ctx.build_alloca_vec_initialized("vec_entries", self.inner.len());

        ctx.build_unconditional_branch(insert_block);
        ctx.position_at_end(insert_block);

        for value in &self.inner {
            let value_ref = ctx.build_alloca_resolved_initialized("value");

            ctx.emit_llvm(
                value,
                value_ref,
                (state.0, state.1),
                end_block,
                vec![
                    (value_ref.into(), ctx.fns().vrl_resolved_drop),
                    (vec_ref.into(), ctx.fns().vrl_vec_drop),
                ],
            )?;

            ctx.fns()
                .vrl_vec_push
                .build_call(ctx.builder(), vec_ref, value_ref);
        }

        ctx.build_unconditional_branch(set_result_block);
        ctx.position_at_end(set_result_block);

        ctx.fns().vrl_expression_array_into_result.build_call(
            ctx.builder(),
            vec_ref,
            ctx.result_ref(),
        );

        ctx.build_unconditional_branch(end_block);
        ctx.position_at_end(end_block);

        Ok(())
    }
}

impl fmt::Display for Array {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let exprs = self
            .inner
            .iter()
            .map(Expr::to_string)
            .collect::<Vec<_>>()
            .join(", ");

        write!(f, "[{}]", exprs)
    }
}

impl From<Vec<Expr>> for Array {
    fn from(inner: Vec<Expr>) -> Self {
        Self { inner }
    }
}

#[cfg(test)]
mod tests {
    use crate::kind::Collection;

    use super::*;
    use crate::{expr, test_type_def, value::Kind, TypeDef};

    test_type_def![
        empty_array {
            expr: |_| expr!([]),
            want: TypeDef::array(Collection::empty()),
        }

        scalar_array {
            expr: |_| expr!([1, "foo", true]),
            want: TypeDef::array(BTreeMap::from([
                (0.into(), Kind::integer()),
                (1.into(), Kind::bytes()),
                (2.into(), Kind::boolean()),
            ])),
        }

        mixed_array {
            expr: |_| expr!([1, [true, "foo"], { "bar": null }]),
            want: TypeDef::array(BTreeMap::from([
                (0.into(), Kind::integer()),
                (1.into(), Kind::array(BTreeMap::from([
                    (0.into(), Kind::boolean()),
                    (1.into(), Kind::bytes()),
                ]))),
                (2.into(), Kind::object(BTreeMap::from([
                    ("bar".into(), Kind::null())
                ]))),
            ])),
        }
    ];
}
