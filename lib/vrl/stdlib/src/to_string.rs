use vrl::prelude::*;

fn to_string(value: Value) -> std::result::Result<Value, ExpressionError> {
    use chrono::SecondsFormat;
    use Value::*;
    let value = match value {
        v @ Bytes(_) => v,
        Integer(v) => v.to_string().into(),
        Float(v) => v.to_string().into(),
        Boolean(v) => v.to_string().into(),
        Timestamp(v) => v.to_rfc3339_opts(SecondsFormat::AutoSi, true).into(),
        Null => "".into(),
        v => return Err(format!(r#"unable to coerce {} into "string""#, v.kind()).into()),
    };
    Ok(value)
}

#[derive(Clone, Copy, Debug)]
pub struct ToString;

impl Function for ToString {
    fn identifier(&self) -> &'static str {
        "to_string"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[Parameter {
            keyword: "value",
            kind: kind::ANY,
            required: true,
        }]
    }

    fn examples(&self) -> &'static [Example] {
        &[
            Example {
                title: "string",
                source: "to_string(s'foo')",
                result: Ok("foo"),
            },
            Example {
                title: "integer",
                source: "to_string(5)",
                result: Ok("s'5'"),
            },
            Example {
                title: "float",
                source: "to_string(5.6)",
                result: Ok("s'5.6'"),
            },
            Example {
                title: "true",
                source: "to_string(true)",
                result: Ok("s'true'"),
            },
            Example {
                title: "false",
                source: "to_string(false)",
                result: Ok("s'false'"),
            },
            Example {
                title: "null",
                source: "to_string(null)",
                result: Ok(""),
            },
            Example {
                title: "timestamp",
                source: "to_string(t'2020-01-01T00:00:00Z')",
                result: Ok("2020-01-01T00:00:00Z"),
            },
            Example {
                title: "array",
                source: "to_string!([])",
                result: Err(
                    r#"function call error for "to_string" at (0:14): unable to coerce "array" into "string""#,
                ),
            },
            Example {
                title: "object",
                source: "to_string!({})",
                result: Err(
                    r#"function call error for "to_string" at (0:14): unable to coerce "object" into "string""#,
                ),
            },
            Example {
                title: "regex",
                source: "to_string!(r'foo')",
                result: Err(
                    r#"function call error for "to_string" at (0:18): unable to coerce "regex" into "string""#,
                ),
            },
        ]
    }

    fn compile(
        &self,
        _state: &state::Compiler,
        _ctx: &FunctionCompileContext,
        mut arguments: ArgumentList,
    ) -> Compiled {
        let value = arguments.required("value");

        Ok(Box::new(ToStringFn { value }))
    }

    fn call_by_vm(&self, _ctx: &mut Context, args: &mut VmArgumentList) -> Resolved {
        let value = args.required("value");

        to_string(value)
    }
}

#[derive(Debug, Clone)]
struct ToStringFn {
    value: Box<dyn Expression>,
}

impl Expression for ToStringFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let value = self.value.resolve(ctx)?;

        to_string(value)
    }

    fn type_def(&self, state: &state::Compiler) -> TypeDef {
        self.value
            .type_def(state)
            .fallible_unless(
                Kind::Bytes
                    | Kind::Integer
                    | Kind::Float
                    | Kind::Boolean
                    | Kind::Null
                    | Kind::Timestamp,
            )
            .bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    test_function![
        to_string => ToString;

        integer {
            args: func_args![value: 20],
            want: Ok("20"),
            tdef: TypeDef::new().bytes(),
        }

        float {
            args: func_args![value: 20.5],
            want: Ok("20.5"),
            tdef: TypeDef::new().bytes(),
        }
    ];
}
