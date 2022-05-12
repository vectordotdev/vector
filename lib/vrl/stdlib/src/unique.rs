use ::value::Value;
use indexmap::IndexSet;
use vrl::prelude::*;

fn unique(value: Value) -> Resolved {
    let value = value.try_array()?;
    let set: IndexSet<_> = value.into_iter().collect();
    Ok(set.into_iter().collect())
}

#[derive(Clone, Copy, Debug)]
pub struct Unique;

impl Function for Unique {
    fn identifier(&self) -> &'static str {
        "unique"
    }

    fn examples(&self) -> &'static [Example] {
        &[Example {
            title: "unique",
            source: r#"unique(["foo", "bar", "foo", "baz"])"#,
            result: Ok(r#"["foo", "bar", "baz"]"#),
        }]
    }

    fn compile(
        &self,
        _state: (&mut state::LocalEnv, &mut state::ExternalEnv),
        _ctx: &mut FunctionCompileContext,
        mut arguments: ArgumentList,
    ) -> Compiled {
        let value = arguments.required("value");

        Ok(Box::new(UniqueFn { value }))
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[Parameter {
            keyword: "value",
            kind: kind::ARRAY,
            required: true,
        }]
    }

    fn call_by_vm(&self, _ctx: &mut Context, args: &mut VmArgumentList) -> Resolved {
        let value = args.required("value");
        unique(value)
    }
}

#[derive(Debug, Clone)]
pub(crate) struct UniqueFn {
    value: Box<dyn Expression>,
}

impl Expression for UniqueFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let value = self.value.resolve(ctx)?;
        unique(value)
    }

    fn type_def(&self, _: (&state::LocalEnv, &state::ExternalEnv)) -> TypeDef {
        TypeDef::array(Collection::any())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    test_function![
        unique => Unique;

        default {
            args: func_args![
                value: value!(["bar", "foo", "baz", "foo"]),
            ],
            want: Ok(value!(["bar", "foo", "baz"])),
            tdef: TypeDef::array(Collection::any()),
        }

        mixed_values {
            args: func_args![
                value: value!(["foo", [1,2,3], "123abc", 1, true, [1,2,3], "foo", true, 1]),
            ],
            want: Ok(value!(["foo", [1,2,3], "123abc", 1, true])),
            tdef: TypeDef::array(Collection::any()),
        }
    ];
}
