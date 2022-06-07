use vrl::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct Exists;

impl Function for Exists {
    fn identifier(&self) -> &'static str {
        "exists"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[Parameter {
            keyword: "field",
            kind: kind::ANY,
            required: true,
        }]
    }

    fn examples(&self) -> &'static [Example] {
        &[
            Example {
                title: "existing field",
                source: r#"exists({ "foo": "bar"}.foo)"#,
                result: Ok("true"),
            },
            Example {
                title: "non-existing field",
                source: r#"exists({ "foo": "bar"}.baz)"#,
                result: Ok("false"),
            },
        ]
    }

    fn compile_argument(
        &self,
        _args: &[(&'static str, Option<ResolvedArgument>)],
        _ctx: &mut FunctionCompileContext,
        name: &str,
        expr: Option<&expression::Expr>,
    ) -> CompiledArgument {
        match (name, expr) {
            ("field", Some(expr)) => {
                let query = match expr {
                    expression::Expr::Query(query) => query,
                    _ => {
                        return Err(Box::new(vrl::function::Error::UnexpectedExpression {
                            keyword: "field",
                            expected: "query",
                            expr: expr.clone(),
                        }))
                    }
                };

                Ok(Some(Box::new(query.clone()) as _))
            }
            _ => Ok(None),
        }
    }

    fn compile(
        &self,
        _state: (&mut state::LocalEnv, &mut state::ExternalEnv),
        _ctx: &mut FunctionCompileContext,
        mut arguments: ArgumentList,
    ) -> Compiled {
        let query = arguments.required_query("field")?;

        Ok(Box::new(ExistsFn { query }))
    }

    fn symbol(&self) -> Option<(&'static str, usize)> {
        None
    }
}

#[derive(Clone, Debug)]
pub(crate) struct ExistsFn {
    query: expression::Query,
}

fn exists(query: &expression::Query, ctx: &mut Context) -> Resolved {
    let path = query.path();

    if query.is_external() {
        return Ok(ctx
            .target_mut()
            .target_get(path)
            .ok()
            .flatten()
            .is_some()
            .into());
    }

    if let Some(ident) = query.variable_ident() {
        return match ctx.state().variable(ident) {
            Some(value) => Ok(value.get_by_path(path).is_some().into()),
            None => Ok(false.into()),
        };
    }

    if let Some(expr) = query.expression_target() {
        let value = expr.resolve(ctx)?;

        return Ok(value.get_by_path(path).is_some().into());
    }

    Ok(false.into())
}

impl Expression for ExistsFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        exists(&self.query, ctx)
    }

    fn type_def(&self, _: (&state::LocalEnv, &state::ExternalEnv)) -> TypeDef {
        TypeDef::boolean().infallible()
    }

    fn emit_llvm<'ctx>(
        &self,
        state: (&mut vrl::state::LocalEnv, &mut vrl::state::ExternalEnv),
        ctx: &mut vrl::llvm::Context<'ctx>,
        function_call_abort_stack: &mut Vec<vrl::llvm::BasicBlock<'ctx>>,
    ) -> std::result::Result<(), String> {
        let query = &self.query;
        let path = query.path();
        let path_ref = ctx.into_lookup_buf_const_ref(path.clone());

        let result_ref = ctx.result_ref();

        if query.is_external() {
            let vrl_exists_external = ctx.vrl_exists_external();
            vrl_exists_external.build_call(
                ctx.builder(),
                ctx.context_ref(),
                ctx.builder().build_bitcast(
                    path_ref,
                    vrl_exists_external
                        .function
                        .get_nth_param(1)
                        .unwrap()
                        .get_type()
                        .into_pointer_type(),
                    "cast",
                ),
                result_ref,
            );
        } else if let Some(ident) = query.variable_ident() {
            let variable_ref = ctx.get_variable_ref(&ident);
            let vrl_exists_internal = ctx.vrl_exists_internal();
            vrl_exists_internal.build_call(
                ctx.builder(),
                variable_ref,
                ctx.builder().build_bitcast(
                    path_ref,
                    vrl_exists_internal
                        .function
                        .get_nth_param(1)
                        .unwrap()
                        .get_type()
                        .into_pointer_type(),
                    "cast",
                ),
                result_ref,
            );
        } else if let Some(expr) = query.expression_target() {
            let resolved_temp_ref = ctx.build_alloca_resolved("temp");
            ctx.vrl_resolved_initialize()
                .build_call(ctx.builder(), resolved_temp_ref);
            ctx.set_result_ref(resolved_temp_ref);
            let mut error_stack = Vec::new();
            expr.emit_llvm(state, ctx, &mut error_stack)?;
            function_call_abort_stack.extend(error_stack);
            let vrl_exists_expression = ctx.vrl_exists_expression();
            vrl_exists_expression.build_call(
                ctx.builder(),
                resolved_temp_ref,
                ctx.builder().build_bitcast(
                    path_ref,
                    vrl_exists_expression
                        .function
                        .get_nth_param(1)
                        .unwrap()
                        .get_type()
                        .into_pointer_type(),
                    "cast",
                ),
                result_ref,
            );
            ctx.set_result_ref(result_ref);
        } else {
            ctx.vrl_resolved_set_false()
                .build_call(ctx.builder(), result_ref);
        }

        Ok(())
    }
}
