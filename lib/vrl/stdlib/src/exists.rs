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
        _args: &[(&'static str, Option<FunctionArgument>)],
        _info: &FunctionCompileContext,
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

    fn call_by_vm(&self, ctx: &mut Context, args: &mut VmArgumentList) -> Resolved {
        let field = args
            .required_any("field")
            .downcast_ref::<expression::Query>()
            .unwrap();

        exists(field, ctx)
    }

    fn compile(
        &self,
        _state: &state::Compiler,
        _ctx: &FunctionCompileContext,
        mut arguments: ArgumentList,
    ) -> Compiled {
        let query = arguments.required_query("field")?;

        Ok(Box::new(ExistsFn { query }))
    }
}

#[derive(Clone, Debug)]
pub struct ExistsFn {
    query: expression::Query,
}

fn exists(query: &expression::Query, ctx: &mut Context) -> Resolved {
    let path = query.path();

    if query.is_external() {
        return Ok(ctx.target_mut().get(path).ok().flatten().is_some().into());
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

    fn type_def(&self, _state: &state::Compiler) -> TypeDef {
        TypeDef::new().infallible().boolean()
    }
}
