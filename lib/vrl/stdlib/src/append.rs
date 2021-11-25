use vrl::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct Append;

impl Function for Append {
    fn identifier(&self) -> &'static str {
        "append"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "value",
                kind: kind::ARRAY,
                required: true,
            },
            Parameter {
                keyword: "items",
                kind: kind::ARRAY,
                required: true,
            },
        ]
    }

    fn examples(&self) -> &'static [Example] {
        &[Example {
            title: "append array",
            source: r#"append([0, 1], [2, 3])"#,
            result: Ok("[0, 1, 2, 3]"),
        }]
    }

    fn compile(
        &self,
        _state: &state::Compiler,
        _ctx: &FunctionCompileContext,
        mut arguments: ArgumentList,
    ) -> Compiled {
        let value = arguments.required("value");
        let items = arguments.required("items");

        Ok(Box::new(AppendFn { value, items }))
    }
}

#[derive(Debug, Clone)]
struct AppendFn {
    value: Box<dyn Expression>,
    items: Box<dyn Expression>,
}

impl Expression for AppendFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let appended = self
            .value
            .resolve(ctx)?
            .borrow()
            .try_array()?
            .iter()
            .chain(self.items.resolve(ctx)?.borrow().try_array()?.iter())
            .cloned()
            .collect::<Value>();

        Ok(SharedValue::from(appended))
    }

    fn type_def(&self, state: &state::Compiler) -> TypeDef {
        self.value.type_def(state).merge(self.items.type_def(state))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    test_function![
        append => Append;

        both_arrays_empty {
            args: func_args![value: shared_value!([]), items: shared_value!([])],
            want: Ok(shared_value!([])),
            tdef: TypeDef::new().array::<TypeDef>(vec![]),
        }

        one_array_empty {
            args: func_args![value: shared_value!([]), items: shared_value!([1, 2, 3])],
            want: Ok(shared_value!([1, 2, 3])),
            tdef: TypeDef::new().array_mapped::<i32, TypeDef>(map! {
                0: Kind::Integer,
                1: Kind::Integer,
                2: Kind::Integer,
            }),
        }

        neither_array_empty {
            args: func_args![value: shared_value!([1, 2, 3]), items: shared_value!([4, 5, 6])],
            want: Ok(shared_value!([1, 2, 3, 4, 5, 6])),
            tdef: TypeDef::new().array_mapped::<i32, TypeDef>(map! {
                0: Kind::Integer,
                1: Kind::Integer,
                2: Kind::Integer,
                3: Kind::Integer,
                4: Kind::Integer,
                5: Kind::Integer,
            }),
        }

        mixed_array_types {
            args: func_args![value: shared_value!([1, 2, 3]), items: shared_value!([true, 5.0, "bar"])],
            want: Ok(shared_value!([1, 2, 3, true, 5.0, "bar"])),
            tdef: TypeDef::new().array_mapped::<i32, TypeDef>(map! {
                0: Kind::Integer,
                1: Kind::Integer,
                2: Kind::Integer,
                3: Kind::Boolean,
                4: Kind::Float,
                5: Kind::Bytes,
            }),
        }
    ];
}
