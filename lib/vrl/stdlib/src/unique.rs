use vrl::prelude::*;

use indexmap::IndexSet;

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
        _state: &state::Compiler,
        _ctx: &FunctionCompileContext,
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
}

#[derive(Debug, Clone)]
pub(crate) struct UniqueFn {
    value: Box<dyn Expression>,
}

impl Expression for UniqueFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let value = self.value.resolve(ctx)?;
        let value = value.borrow();
        let value = value.try_array()?;

        let set: IndexSet<_> = value.into_iter().cloned().collect();

        Ok(set.into_iter().collect())
    }

    fn type_def(&self, _: &state::Compiler) -> TypeDef {
        TypeDef::new().array_mapped::<(), Kind>(map! { (): Kind::all() })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    test_function![
        unique => Unique;

        default {
            args: func_args![
                value: shared_value!(["bar", "foo", "baz", "foo"]),
            ],
            want: Ok(shared_value!(["bar", "foo", "baz"])),
            tdef: TypeDef::new().array_mapped::<(), Kind>(map! { (): Kind::all() }),
        }

        mixed_values {
            args: func_args![
                value: shared_value!(["foo", [1,2,3], "123abc", 1, true, [1,2,3], "foo", true, 1]),
            ],
            want: Ok(shared_value!(["foo", [1,2,3], "123abc", 1, true])),
            tdef: TypeDef::new().array_mapped::<(), Kind>(map! { (): Kind::all() }),
        }
    ];
}
