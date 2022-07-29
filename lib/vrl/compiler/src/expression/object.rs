use std::{collections::BTreeMap, fmt, ops::Deref};

use value::Value;

use crate::{
    expression::{Expr, Resolved},
    state::{ExternalEnv, LocalEnv},
    Context, Expression, TypeDef,
};

#[derive(Debug, Clone, PartialEq)]
pub struct Object {
    inner: BTreeMap<String, Expr>,
}

impl Object {
    #[must_use]
    pub fn new(inner: BTreeMap<String, Expr>) -> Self {
        Self { inner }
    }
}

impl Deref for Object {
    type Target = BTreeMap<String, Expr>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl Expression for Object {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        self.inner
            .iter()
            .map(|(key, expr)| expr.resolve(ctx).map(|v| (key.clone(), v)))
            .collect::<Result<BTreeMap<_, _>, _>>()
            .map(Value::Object)
    }

    fn as_value(&self) -> Option<Value> {
        self.inner
            .iter()
            .map(|(key, expr)| expr.as_value().map(|v| (key.clone(), v)))
            .collect::<Option<BTreeMap<_, _>>>()
            .map(Value::Object)
    }

    fn type_def(&self, state: (&LocalEnv, &ExternalEnv)) -> TypeDef {
        let type_defs = self
            .inner
            .iter()
            .map(|(k, expr)| (k.clone(), expr.type_def(state)))
            .collect::<BTreeMap<_, _>>();

        // If any of the stored expressions is fallible, the entire object is
        // fallible.
        let fallible = type_defs.values().any(TypeDef::is_fallible);

        let abortable = type_defs.values().any(TypeDef::is_abortable);

        let collection = type_defs
            .into_iter()
            .map(|(field, type_def)| (field.into(), type_def.into()))
            .collect::<BTreeMap<_, _>>();

        TypeDef::object(collection)
            .with_fallibility(fallible)
            .with_abortability(abortable)
    }

    #[cfg(feature = "llvm")]
    fn emit_llvm<'ctx>(
        &self,
        state: (&mut LocalEnv, &mut ExternalEnv),
        ctx: &mut crate::llvm::Context<'ctx>,
    ) -> Result<(), String> {
        let begin_block = ctx.append_basic_block("object_begin");
        let end_block = ctx.append_basic_block("object_end");
        let insert_block = ctx.append_basic_block("object_insert");
        let set_result_block = ctx.append_basic_block("object_set_result");

        ctx.build_unconditional_branch(begin_block);
        ctx.position_at_end(begin_block);

        let btree_map_ref =
            ctx.build_alloca_btree_map_initialized("btree_map_entries", self.inner.len());

        ctx.build_unconditional_branch(insert_block);
        ctx.position_at_end(insert_block);

        for (key, expression) in &self.inner {
            let key_ref = ctx.into_const(key.clone(), key).as_pointer_value();
            let entry_ref = ctx.build_alloca_resolved_initialized("object_entry");

            ctx.emit_llvm(
                expression,
                entry_ref,
                (state.0, state.1),
                end_block,
                vec![
                    (entry_ref.into(), ctx.fns().vrl_resolved_drop),
                    (btree_map_ref.into(), ctx.fns().vrl_btree_map_drop),
                ],
            )?;

            ctx.fns().vrl_btree_map_push.build_call(
                ctx.builder(),
                btree_map_ref,
                ctx.cast_string_ref_type(key_ref),
                entry_ref,
            );
        }

        ctx.build_unconditional_branch(set_result_block);
        ctx.position_at_end(set_result_block);

        ctx.fns().vrl_expression_object_into_result.build_call(
            ctx.builder(),
            btree_map_ref,
            ctx.result_ref(),
        );

        ctx.build_unconditional_branch(end_block);
        ctx.position_at_end(end_block);

        Ok(())
    }
}

impl fmt::Display for Object {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let exprs = self
            .inner
            .iter()
            .map(|(k, v)| format!(r#""{}": {}"#, k, v))
            .collect::<Vec<_>>()
            .join(", ");

        write!(f, "{{ {} }}", exprs)
    }
}

impl From<BTreeMap<String, Expr>> for Object {
    fn from(inner: BTreeMap<String, Expr>) -> Self {
        Self { inner }
    }
}
