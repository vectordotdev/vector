use vrl::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct IsTimestamp;

impl Function for IsTimestamp {
    fn identifier(&self) -> &'static str {
        "is_timestamp"
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
                source: r#"is_timestamp("foobar")"#,
                result: Ok("false"),
            },
            Example {
                title: "boolean",
                source: r#"is_timestamp(true)"#,
                result: Ok("false"),
            },
            Example {
                title: "null",
                source: r#"is_timestamp(t'2021-03-26T16:00:00Z')"#,
                result: Ok("true"),
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

        Ok(Box::new(IsTimestampFn { value }))
    }

    fn call_by_vm(&self, _ctx: &mut Context, args: &mut VmArgumentList) -> Resolved {
        Ok(value!(args.required("value").is_timestamp()))
    }
}

#[derive(Clone, Debug)]
struct IsTimestampFn {
    value: Box<dyn Expression>,
}

impl Expression for IsTimestampFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        self.value.resolve(ctx).map(|v| value!(v.is_timestamp()))
    }

    fn type_def(&self, _: &state::Compiler) -> TypeDef {
        TypeDef::new().infallible().boolean()
    }
}

#[cfg(test)]
mod tests {
    use chrono::{DateTime, Utc};

    use super::*;

    test_function![
        is_timestamp => IsTimestamp;

        timestamp {
            args: func_args![value: value!(DateTime::parse_from_rfc2822("Wed, 17 Mar 2021 12:00:00 +0000")
                .unwrap()
                .with_timezone(&Utc))],
            want: Ok(value!(true)),
            tdef: TypeDef::new().infallible().boolean(),
        }

        integer {
            args: func_args![value: value!(1789)],
            want: Ok(value!(false)),
            tdef: TypeDef::new().infallible().boolean(),
        }
    ];
}
