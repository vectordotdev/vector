use ::value::Value;
use vrl::prelude::*;

fn tally_value(array: Value, value: Value) -> Resolved {
    let array = array.try_array()?;
    Ok(array.iter().filter(|&v| v == &value).count().into())
}

#[derive(Clone, Copy, Debug)]
pub struct TallyValue;

impl Function for TallyValue {
    fn identifier(&self) -> &'static str {
        "tally_value"
    }

    fn examples(&self) -> &'static [Example] {
        &[Example {
            title: "count matching values",
            source: r#"tally_value(["foo", "bar", "foo", "baz"], "foo")"#,
            result: Ok("2"),
        }]
    }

    fn compile(
        &self,
        _state: (&mut state::LocalEnv, &mut state::ExternalEnv),
        _ctx: &mut FunctionCompileContext,
        mut arguments: ArgumentList,
    ) -> Compiled {
        let array = arguments.required("array");
        let value = arguments.required("value");

        Ok(Box::new(TallyValueFn { array, value }))
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "array",
                kind: kind::ARRAY,
                required: true,
            },
            Parameter {
                keyword: "value",
                kind: kind::ANY,
                required: true,
            },
        ]
    }

    fn call_by_vm(&self, _ctx: &mut Context, args: &mut VmArgumentList) -> Resolved {
        let array = args.required("array");
        let value = args.required("value");

        tally_value(array, value)
    }
}

#[derive(Debug, Clone)]
pub(crate) struct TallyValueFn {
    array: Box<dyn Expression>,
    value: Box<dyn Expression>,
}

impl Expression for TallyValueFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let array = self.array.resolve(ctx)?;
        let value = self.value.resolve(ctx)?;

        tally_value(array, value)
    }

    fn type_def(&self, _: (&state::LocalEnv, &state::ExternalEnv)) -> TypeDef {
        TypeDef::integer().infallible()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    test_function![
        tally_value => TallyValue;

        default {
            args: func_args![
                array: value!(["bar", "foo", "baz", "foo"]),
                value: value!("foo"),
            ],
            want: Ok(value!(2)),
            tdef: TypeDef::integer().infallible(),
        }
    ];
}
