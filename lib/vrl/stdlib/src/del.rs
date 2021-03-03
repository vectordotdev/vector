use vrl::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct Del;

impl Function for Del {
    fn identifier(&self) -> &'static str {
        "del"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[Parameter {
            keyword: "target",
            kind: kind::ANY,
            required: true,
        }]
    }

    fn examples(&self) -> &'static [Example] {
        &[
            Example {
                title: "returns deleted field",
                source: r#"del({"foo": "bar"}.foo)"#,
                result: Ok("bar"),
            },
            Example {
                title: "returns null for unknown field",
                source: r#"del({"foo": "bar"}.baz)"#,
                result: Ok("null"),
            },
            Example {
                title: "external target",
                source: indoc! {r#"
                    . = { "foo": true, "bar": 10 }
                    del(.foo)
                    .
                "#},
                result: Ok(r#"{ "bar": 10 }"#),
            },
            Example {
                title: "variable",
                source: indoc! {r#"
                    var = { "foo": true, "bar": 10 }
                    del(var.foo)
                    var
                "#},
                result: Ok(r#"{ "bar": 10 }"#),
            },
        ]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Compiled {
        let query = arguments.required_query("target")?;

        Ok(Box::new(DelFn { query }))
    }
}

#[derive(Debug, Clone)]
pub struct DelFn {
    query: expression::Query,
}

impl Expression for DelFn {
    // TODO: we're silencing the result of the `remove` call here, to make this
    // function infallible.
    //
    // This isn't correct though, since, while deleting Vector log fields is
    // infallible, deleting metric fields is not.
    //
    // For example, if you try to delete `.name` in a metric event, the call
    // returns an error, since this is an immutable field.
    //
    // After some debating, we've decided to _silently ignore_ deletions of
    // immutable fields for now, but we'll circle back to this in the near
    // future to potentially improve this situation.
    //
    // see tracking issue: https://github.com/timberio/vector/issues/5887
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let path = self.query.path();

        if self.query.is_external() {
            return Ok(ctx
                .target_mut()
                .remove(path, false)
                .ok()
                .flatten()
                .unwrap_or(Value::Null));
        }

        if let Some(ident) = self.query.variable_ident() {
            return match ctx.state_mut().variable_mut(ident) {
                Some(value) => {
                    let new_value = value.get_by_path(path).cloned();
                    value.remove_by_path(path, false);
                    Ok(new_value.unwrap_or(Value::Null))
                }
                None => Ok(Value::Null),
            };
        }

        if let Some(expr) = self.query.expression_target() {
            let value = expr.resolve(ctx)?;

            return Ok(value.get_by_path(path).cloned().unwrap_or(Value::Null));
        }

        Ok(Value::Null)
    }

    fn type_def(&self, _: &state::Compiler) -> TypeDef {
        TypeDef::new().unknown()
    }
}

impl fmt::Display for DelFn {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("")
    }
}
