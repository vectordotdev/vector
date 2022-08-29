use vrl::prelude::expression::FunctionExpression;
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

    fn compile(
        &self,
        _state: &state::TypeState,
        _ctx: &mut FunctionCompileContext,
        mut arguments: ArgumentList,
    ) -> Compiled {
        let query = arguments.required_query("field")?;

        Ok(ExistsFn { query }.as_expr())
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

impl FunctionExpression for ExistsFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        exists(&self.query, ctx)
    }

    fn type_def(&self, _: &state::TypeState) -> TypeDef {
        TypeDef::boolean().infallible()
    }
}
