use crate::expression::{Expr, Resolved};
use crate::vm::OpCode;
use crate::{Context, Expression, State, TypeDef, Value};
use std::collections::BTreeMap;
use std::{fmt, ops::Deref};

use super::Literal;

#[derive(Debug, Clone, PartialEq)]
pub struct Object {
    inner: BTreeMap<String, Expr>,
}

impl Object {
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
            .map(|(key, expr)| expr.resolve(ctx).map(|v| (key.to_owned(), v)))
            .collect::<Result<BTreeMap<_, _>, _>>()
            .map(Value::Object)
    }

    fn as_value(&self) -> Option<Value> {
        self.inner
            .iter()
            .map(|(key, expr)| expr.as_value().map(|v| (key.to_owned(), v)))
            .collect::<Option<BTreeMap<_, _>>>()
            .map(Value::Object)
    }

    fn type_def(&self, state: &State) -> TypeDef {
        let type_defs = self
            .inner
            .iter()
            .map(|(k, expr)| (k.to_owned(), expr.type_def(state)))
            .collect::<BTreeMap<_, _>>();

        // If any of the stored expressions is fallible, the entire object is
        // fallible.
        let fallible = type_defs.values().any(TypeDef::is_fallible);

        TypeDef::new().object(type_defs).with_fallibility(fallible)
    }

    fn dump(&self, vm: &mut crate::vm::Vm) -> Result<(), String> {
        for (key, value) in &self.inner {
            let keyidx = vm.add_constant(Literal::String(key.clone().into()));

            vm.write_chunk(OpCode::Constant);
            vm.write_primitive(keyidx);

            value.dump(vm)?;
        }

        vm.write_chunk(OpCode::CreateObject);
        vm.write_primitive(self.inner.len());

        Ok(())
    }

    #[cfg(feature = "llvm")]
    fn emit_llvm<'ctx>(&self, ctx: &mut crate::llvm::Context<'ctx>) -> Result<(), String> {
        let function = ctx.function();
        let begin_block = ctx.context().append_basic_block(function, "object_begin");
        ctx.builder().build_unconditional_branch(begin_block);
        ctx.builder().position_at_end(begin_block);

        let end_block = ctx.context().append_basic_block(function, "object_end");
        let error_block = ctx.context().append_basic_block(function, "object_error");

        let btree_map_type_identifier =
            "alloc::collections::btree::map::BTreeMap<u64, read::abbrev::Abbreviation>";
        let btree_map_type = ctx
            .module()
            .get_struct_type(btree_map_type_identifier)
            .ok_or(format!(
                r#"failed getting type "{}" from module"#,
                btree_map_type_identifier
            ))?;
        let btree_map_ref = ctx.builder().build_alloca(btree_map_type, "temp");

        {
            let fn_ident = "vrl_btree_map_initialize";
            let fn_impl = ctx
                .module()
                .get_function(fn_ident)
                .ok_or(format!(r#"failed to get "{}" function"#, fn_ident))?;
            ctx.builder()
                .build_call(fn_impl, &[btree_map_ref.into()], fn_ident)
        };
        {
            let fn_ident = "vrl_resolved_initialize";
            let fn_impl = ctx
                .module()
                .get_function(fn_ident)
                .ok_or(format!(r#"failed to get "{}" function"#, fn_ident))?;
            ctx.builder()
                .build_call(fn_impl, &[ctx.result_ref().into()], fn_ident)
        };

        let insert_block = ctx.context().append_basic_block(function, "object_insert");
        ctx.builder().build_unconditional_branch(insert_block);
        ctx.builder().position_at_end(insert_block);

        for (key, expr) in &self.inner {
            expr.emit_llvm(ctx)?;

            let is_err = {
                let fn_ident = "vrl_resolved_is_err";
                let fn_impl = ctx
                    .module()
                    .get_function(fn_ident)
                    .ok_or(format!(r#"failed to get "{}" function"#, fn_ident))?;
                ctx.builder()
                    .build_call(fn_impl, &[ctx.result_ref().into()], fn_ident)
                    .try_as_basic_value()
                    .left()
                    .ok_or(format!(r#"result of "{}" is not a basic value"#, fn_ident))?
                    .try_into()
                    .map_err(|_| format!(r#"result of "{}" is not an int value"#, fn_ident))?
            };

            let insert_block = ctx.context().append_basic_block(function, "object_insert");
            ctx.builder()
                .build_conditional_branch(is_err, error_block, insert_block);
            ctx.builder().position_at_end(insert_block);

            let key_ref = ctx.into_const(key.clone(), key).as_pointer_value();

            {
                let fn_ident = "vrl_btree_map_insert";
                let fn_impl = ctx
                    .module()
                    .get_function(fn_ident)
                    .ok_or(format!(r#"failed to get "{}" function"#, fn_ident))?;
                ctx.builder().build_call(
                    fn_impl,
                    &[
                        btree_map_ref.into(),
                        ctx.builder()
                            .build_bitcast(
                                key_ref,
                                fn_impl
                                    .get_nth_param(1)
                                    .unwrap()
                                    .get_type()
                                    .into_pointer_type(),
                                "cast",
                            )
                            .into(),
                        ctx.result_ref().into(),
                    ],
                    fn_ident,
                )
            };
        }

        let set_result_block = ctx
            .context()
            .append_basic_block(function, "object_set_result");
        ctx.builder().build_unconditional_branch(set_result_block);
        ctx.builder().position_at_end(set_result_block);

        {
            let fn_ident = "vrl_expression_object_set_result_impl";
            let fn_impl = ctx
                .module()
                .get_function(fn_ident)
                .ok_or(format!(r#"failed to get "{}" function"#, fn_ident))?;
            ctx.builder().build_call(
                fn_impl,
                &[btree_map_ref.into(), ctx.result_ref().into()],
                fn_ident,
            )
        };

        ctx.builder().build_unconditional_branch(end_block);

        ctx.builder().position_at_end(error_block);
        ctx.builder().build_unconditional_branch(end_block);

        ctx.builder().position_at_end(end_block);

        {
            let fn_ident = "vrl_btree_map_drop";
            let fn_impl = ctx
                .module()
                .get_function(fn_ident)
                .ok_or(format!(r#"failed to get "{}" function"#, fn_ident))?;
            ctx.builder()
                .build_call(fn_impl, &[btree_map_ref.into()], fn_ident)
        };

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
