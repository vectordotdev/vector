use vrl::prelude::*;

fn del(
    query: &expression::Query,
    ctx: &mut Context,
) -> std::result::Result<Value, ExpressionError> {
    let path = query.path();
    if query.is_external() {
        return Ok(ctx
            .target_mut()
            .remove(path, false)
            .ok()
            .flatten()
            .unwrap_or(Value::Null));
    }
    if let Some(ident) = query.variable_ident() {
        return match ctx.state_mut().variable_mut(ident) {
            Some(value) => {
                let new_value = value.get_by_path(path).cloned();
                value.remove_by_path(path, false);
                Ok(new_value.unwrap_or(Value::Null))
            }
            None => Ok(Value::Null),
        };
    }
    if let Some(expr) = query.expression_target() {
        let value = expr.resolve(ctx)?;

        return Ok(value.get_by_path(path).cloned().unwrap_or(Value::Null));
    }
    Ok(Value::Null)
}

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

    fn compile(
        &self,
        _state: &state::Compiler,
        _ctx: &FunctionCompileContext,
        mut arguments: ArgumentList,
    ) -> Compiled {
        let query = arguments.required_query("target")?;

        Ok(Box::new(DelFn { query }))
    }

    fn compile_argument(
        &self,
        _args: &[(&'static str, Option<FunctionArgument>)],
        _info: &FunctionCompileContext,
        name: &str,
        expr: Option<&expression::Expr>,
    ) -> CompiledArgument {
        match (name, expr) {
            ("target", Some(expr)) => {
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
        let query = args
            .required_any("target")
            .downcast_ref::<expression::Query>()
            .unwrap();

        del(query, ctx)
    }
}

#[derive(Debug, Clone)]
pub struct DelFn {
    query: expression::Query,
}

impl DelFn {
    #[cfg(test)]
    fn new(path: &str) -> Self {
        use std::str::FromStr;

        Self {
            query: expression::Query::new(
                expression::Target::External,
                FromStr::from_str(path).unwrap(),
            ),
        }
    }
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
        del(&self.query, ctx)
    }

    fn type_def(&self, _: &state::Compiler) -> TypeDef {
        TypeDef::new().unknown()
    }

    fn update_state(
        &mut self,
        state: &mut state::Compiler,
    ) -> std::result::Result<(), ExpressionError> {
        self.query.delete_type_def(state);
        Ok(())
    }
}

impl fmt::Display for DelFn {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("")
    }
}

#[cfg(test)]
mod tests {
    use vector_common::{btreemap, TimeZone};

    use super::*;

    #[test]
    fn del() {
        let cases = vec![
            (
                // String field exists
                btreemap! { "exists" => "value" },
                Ok(value!("value")),
                DelFn::new("exists"),
            ),
            (
                // String field doesn't exist
                btreemap! { "exists" => "value" },
                Ok(value!(null)),
                DelFn::new("does_not_exist"),
            ),
            (
                // Array field exists
                btreemap! { "exists" => value!([1, 2, 3]) },
                Ok(value!([1, 2, 3])),
                DelFn::new("exists"),
            ),
            (
                // Null field exists
                btreemap! { "exists" => value!(null) },
                Ok(value!(null)),
                DelFn::new("exists"),
            ),
            (
                // Map field exists
                btreemap! {"exists" => btreemap! { "foo" => "bar" }},
                Ok(value!(btreemap! {"foo" => "bar" })),
                DelFn::new("exists"),
            ),
            (
                // Integer field exists
                btreemap! { "exists" => 127 },
                Ok(value!(127)),
                DelFn::new("exists"),
            ),
            (
                // Array field exists
                btreemap! {"exists" => value!([1, 2, 3]) },
                Ok(value!(2)),
                DelFn::new(".exists[1]"),
            ),
        ];
        let tz = TimeZone::default();
        for (object, exp, func) in cases {
            let mut object: Value = object.into();
            let mut runtime_state = vrl::state::Runtime::default();
            let mut ctx = Context::new(&mut object, &mut runtime_state, &tz);
            let got = func
                .resolve(&mut ctx)
                .map_err(|e| format!("{:#}", anyhow::anyhow!(e)));
            assert_eq!(got, exp);
        }
    }
}
