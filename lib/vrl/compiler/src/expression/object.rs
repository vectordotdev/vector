use std::{collections::BTreeMap, fmt, ops::Deref};

use value::Value;

use crate::{
    expression::{Expr, Resolved},
    state::{ExternalEnv, LocalEnv},
    vm::OpCode,
    Context, Expression, TypeDef,
};

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

    fn type_def(&self, state: (&LocalEnv, &ExternalEnv)) -> TypeDef {
        let type_defs = self
            .inner
            .iter()
            .map(|(k, expr)| (k.to_owned(), expr.type_def(state)))
            .collect::<BTreeMap<_, _>>();

        // If any of the stored expressions is fallible, the entire object is
        // fallible.
        let fallible = type_defs.values().any(TypeDef::is_fallible);

        let collection = type_defs
            .into_iter()
            .map(|(field, type_def)| (field.into(), type_def.into()))
            .collect::<BTreeMap<_, _>>();

        TypeDef::object(collection).with_fallibility(fallible)
    }

    fn compile_to_vm(
        &self,
        vm: &mut crate::vm::Vm,
        state: (&mut LocalEnv, &mut ExternalEnv),
    ) -> Result<(), String> {
        let (local, external) = state;

        for (key, value) in &self.inner {
            // Write the key as a constant
            let keyidx = vm.add_constant(Value::Bytes(key.clone().into()));
            vm.write_opcode(OpCode::Constant);
            vm.write_primitive(keyidx);

            // Write the value
            value.compile_to_vm(vm, (local, external))?;
        }

        vm.write_opcode(OpCode::CreateObject);

        // Write the number of key/value pairs in the object so the machine knows
        // how many pairs to suck into the created object.
        vm.write_primitive(self.inner.len());

        Ok(())
    }

    #[cfg(feature = "llvm")]
    fn emit_llvm<'ctx>(
        &self,
        state: (&mut LocalEnv, &mut ExternalEnv),
        ctx: &mut crate::llvm::Context<'ctx>,
    ) -> Result<(), String> {
        let function = ctx.function();
        let begin_block = ctx.context().append_basic_block(function, "object_begin");
        ctx.builder().build_unconditional_branch(begin_block);
        ctx.builder().position_at_end(begin_block);

        let result_ref = ctx.result_ref();

        let end_block = ctx.context().append_basic_block(function, "object_end");

        let btree_map_ref = ctx.builder().build_alloca(ctx.btree_map_type(), "temp");

        {
            let fn_ident = "vrl_btree_map_initialize";
            let fn_impl = ctx
                .module()
                .get_function(fn_ident)
                .ok_or(format!(r#"failed to get "{}" function"#, fn_ident))?;
            ctx.builder().build_call(
                fn_impl,
                &[
                    btree_map_ref.into(),
                    ctx.usize_type()
                        .const_int(self.inner.len() as _, false)
                        .into(),
                ],
                fn_ident,
            );
        }

        let insert_block = ctx.context().append_basic_block(function, "object_insert");
        ctx.builder().build_unconditional_branch(insert_block);
        ctx.builder().position_at_end(insert_block);

        let value_refs = self
            .inner
            .iter()
            .enumerate()
            .map(|(index, _)| ctx.build_alloca_resolved(&format!("value_{}", index)))
            .collect::<Vec<_>>();

        for (index, _) in self.inner.iter().enumerate() {
            let fn_ident = "vrl_resolved_initialize";
            let fn_impl = ctx
                .module()
                .get_function(fn_ident)
                .ok_or(format!(r#"failed to get "{}" function"#, fn_ident))?;
            ctx.builder()
                .build_call(fn_impl, &[value_refs[index].into()], fn_ident);
        }

        for (index, (_, expression)) in self.inner.iter().enumerate() {
            let value_ref = value_refs[index];
            ctx.set_result_ref(value_ref);
            expression.emit_llvm((state.0, state.1), ctx)?;
            let type_def = expression.type_def((state.0, state.1));
            if type_def.is_abortable() {
                let is_err = {
                    let fn_ident = "vrl_resolved_is_err";
                    let fn_impl = ctx
                        .module()
                        .get_function(fn_ident)
                        .ok_or(format!(r#"failed to get "{}" function"#, fn_ident))?;
                    ctx.builder()
                        .build_call(fn_impl, &[value_ref.into()], fn_ident)
                        .try_as_basic_value()
                        .left()
                        .ok_or(format!(r#"result of "{}" is not a basic value"#, fn_ident))?
                        .try_into()
                        .map_err(|_| format!(r#"result of "{}" is not an int value"#, fn_ident))?
                };

                let insert_block = ctx.context().append_basic_block(function, "object_insert");
                ctx.builder()
                    .build_conditional_branch(is_err, end_block, insert_block);
                ctx.builder().position_at_end(insert_block);
            }
        }

        for (index, (key, _)) in self.inner.iter().enumerate() {
            let value_ref = value_refs[index];
            let key_ref = ctx.into_const(key.to_owned(), key).as_pointer_value();

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
                        ctx.usize_type().const_int(index as _, false).into(),
                        ctx.builder()
                            .build_bitcast(
                                key_ref,
                                fn_impl
                                    .get_nth_param(2)
                                    .unwrap()
                                    .get_type()
                                    .into_pointer_type(),
                                "cast",
                            )
                            .into(),
                        value_ref.into(),
                    ],
                    fn_ident,
                )
            };
        }

        ctx.set_result_ref(result_ref);

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
        ctx.builder().position_at_end(end_block);

        for (index, _) in self.inner.iter().enumerate() {
            let fn_ident = "vrl_resolved_drop";
            let fn_impl = ctx
                .module()
                .get_function(fn_ident)
                .ok_or(format!(r#"failed to get "{}" function"#, fn_ident))?;
            ctx.builder()
                .build_call(fn_impl, &[value_refs[index].into()], fn_ident);
        }

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
