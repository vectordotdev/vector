use ::value::Value;
use primitive_calling_convention::primitive_calling_convention;
use vrl::prelude::*;

fn is_timestamp(value: Value) -> Resolved {
    Ok(value.is_timestamp().into())
}

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
        _state: (&mut state::LocalEnv, &mut state::ExternalEnv),
        _ctx: &mut FunctionCompileContext,
        mut arguments: ArgumentList,
    ) -> Compiled {
        let value = arguments.required("value");

        Ok(Box::new(IsTimestampFn { value }))
    }

    fn symbol(&self) -> Option<Symbol> {
        Some(Symbol {
            name: "vrl_fn_is_timestamp",
            address: vrl_fn_is_timestamp as _,
            uses_context: false,
        })
    }
}

#[derive(Clone, Debug)]
struct IsTimestampFn {
    value: Box<dyn Expression>,
}

impl Expression for IsTimestampFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let value = self.value.resolve(ctx)?;

        is_timestamp(value)
    }

    fn type_def(&self, _: (&state::LocalEnv, &state::ExternalEnv)) -> TypeDef {
        TypeDef::boolean().infallible()
    }
}

#[no_mangle]
#[primitive_calling_convention]
extern "C" fn vrl_fn_is_timestamp(value: Value) -> Resolved {
    is_timestamp(value)
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
            tdef: TypeDef::boolean().infallible(),
        }

        integer {
            args: func_args![value: value!(1789)],
            want: Ok(value!(false)),
            tdef: TypeDef::boolean().infallible(),
        }
    ];
}
