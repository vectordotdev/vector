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

    fn compile(&self, mut arguments: ArgumentList) -> Compiled {
        let query = arguments.required_query("field")?;

        Ok(Box::new(ExistsFn { query }))
    }
}

#[derive(Clone, Debug)]
pub struct ExistsFn {
    query: expression::Query,
}

impl Expression for ExistsFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let path = self.query.path();

        if self.query.is_external() {
            return Ok(ctx.target_mut().get(path).ok().flatten().is_some().into());
        }

        if let Some(ident) = self.query.variable_ident() {
            return match ctx.state().variable(ident) {
                Some(value) => Ok(value.get_by_path(path).is_some().into()),
                None => Ok(false.into()),
            };
        }

        if let Some(expr) = self.query.expression_target() {
            let value = expr.resolve(ctx)?;

            return Ok(value.get_by_path(path).is_some().into());
        }

        Ok(false.into())
    }

    fn type_def(&self, _state: &state::Compiler) -> TypeDef {
        TypeDef::new().infallible().boolean()
    }
}
