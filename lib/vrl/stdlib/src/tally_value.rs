use vrl::prelude::*;

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
        _state: &state::Compiler,
        _ctx: &FunctionCompileContext,
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
}

#[derive(Debug, Clone)]
pub(crate) struct TallyValueFn {
    array: Box<dyn Expression>,
    value: Box<dyn Expression>,
}

impl Expression for TallyValueFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let array = self.array.resolve(ctx)?.try_array()?;
        let value = self.value.resolve(ctx)?;

        Ok(array.iter().filter(|&v| v == &value).count().into())
    }

    fn type_def(&self, _: &state::Compiler) -> TypeDef {
        TypeDef::new().infallible().integer()
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
            tdef: TypeDef::new().infallible().integer(),
        }
    ];
}
