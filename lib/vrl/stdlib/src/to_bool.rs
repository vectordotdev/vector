use shared::conversion::Conversion;
use vrl::prelude::*;

fn to_bool(value: Value) -> std::result::Result<Value, ExpressionError> {
    use Value::*;

    match value {
        Boolean(_) => Ok(value),
        Integer(v) => Ok(Boolean(v != 0)),
        Float(v) => Ok(Boolean(v != 0.0)),
        Null => Ok(Boolean(false)),
        Bytes(v) => Conversion::Boolean
            .convert(v)
            .map_err(|e| e.to_string().into()),
        v => Err(format!(r#"unable to coerce {} into "boolean""#, v.kind()).into()),
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ToBool;

impl Function for ToBool {
    fn identifier(&self) -> &'static str {
        "to_bool"
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
                title: "integer (0)",
                source: "to_bool(0)",
                result: Ok("false"),
            },
            Example {
                title: "integer (other)",
                source: "to_bool(2)",
                result: Ok("true"),
            },
            Example {
                title: "float (0)",
                source: "to_bool(0.0)",
                result: Ok("false"),
            },
            Example {
                title: "float (other)",
                source: "to_bool(5.6)",
                result: Ok("true"),
            },
            Example {
                title: "true",
                source: "to_bool(true)",
                result: Ok("true"),
            },
            Example {
                title: "false",
                source: "to_bool(false)",
                result: Ok("false"),
            },
            Example {
                title: "null",
                source: "to_bool(null)",
                result: Ok("false"),
            },
            Example {
                title: "true string",
                source: "to_bool!(s'true')",
                result: Ok("true"),
            },
            Example {
                title: "yes string",
                source: "to_bool!(s'yes')",
                result: Ok("true"),
            },
            Example {
                title: "y string",
                source: "to_bool!(s'y')",
                result: Ok("true"),
            },
            Example {
                title: "non-zero integer string",
                source: "to_bool!(s'1')",
                result: Ok("true"),
            },
            Example {
                title: "false string",
                source: "to_bool!(s'false')",
                result: Ok("false"),
            },
            Example {
                title: "no string",
                source: "to_bool!(s'no')",
                result: Ok("false"),
            },
            Example {
                title: "n string",
                source: "to_bool!(s'n')",
                result: Ok("false"),
            },
            Example {
                title: "zero integer string",
                source: "to_bool!(s'0')",
                result: Ok("false"),
            },
            Example {
                title: "invalid string",
                source: "to_bool!(s'foobar')",
                result: Err(
                    r#"function call error for "to_bool" at (0:19): Invalid boolean value "foobar""#,
                ),
            },
            Example {
                title: "timestamp",
                source: "to_bool!(t'2020-01-01T00:00:00Z')",
                result: Err(
                    r#"function call error for "to_bool" at (0:33): unable to coerce "timestamp" into "boolean""#,
                ),
            },
            Example {
                title: "array",
                source: "to_bool!([])",
                result: Err(
                    r#"function call error for "to_bool" at (0:12): unable to coerce "array" into "boolean""#,
                ),
            },
            Example {
                title: "object",
                source: "to_bool!({})",
                result: Err(
                    r#"function call error for "to_bool" at (0:12): unable to coerce "object" into "boolean""#,
                ),
            },
            Example {
                title: "regex",
                source: "to_bool!(r'foo')",
                result: Err(
                    r#"function call error for "to_bool" at (0:16): unable to coerce "regex" into "boolean""#,
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

        Ok(Box::new(ToBoolFn { value }))
    }

    fn call_by_vm(&self, _ctx: &mut Context, args: &mut VmArgumentList) -> Resolved {
        let value = args.required("value");

        to_bool(value)
    }
}

#[derive(Debug, Clone)]
struct ToBoolFn {
    value: Box<dyn Expression>,
}

impl Expression for ToBoolFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let value = self.value.resolve(ctx)?;

        to_bool(value)
    }

    fn type_def(&self, state: &state::Compiler) -> TypeDef {
        TypeDef::new()
            .with_fallibility(
                self.value.type_def(state).has_kind(
                    Kind::Bytes | Kind::Timestamp | Kind::Array | Kind::Object | Kind::Regex,
                ),
            )
            .boolean()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    test_function![
        to_bool => ToBool;

        string_true {
            args: func_args![value: "true"],
            want: Ok(true),
            tdef: TypeDef::new().fallible().boolean(),
        }

        string_false {
            args: func_args![value: "no"],
            want: Ok(false),
            tdef: TypeDef::new().fallible().boolean(),
        }

        string_error {
            args: func_args![value: "cabbage"],
            want: Err(r#"Invalid boolean value "cabbage""#),
            tdef: TypeDef::new().fallible().boolean(),
        }

        number_true {
            args: func_args![value: 20],
            want: Ok(true),
            tdef: TypeDef::new().infallible().boolean(),
        }

        number_false {
            args: func_args![value: 0],
            want: Ok(false),
            tdef: TypeDef::new().infallible().boolean(),
        }
    ];
}
