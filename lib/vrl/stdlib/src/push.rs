use vrl::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct Push;

impl Function for Push {
    fn identifier(&self) -> &'static str {
        "push"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "value",
                kind: kind::ARRAY,
                required: true,
            },
            Parameter {
                keyword: "item",
                kind: kind::ANY,
                required: true,
            },
        ]
    }

    fn examples(&self) -> &'static [Example] {
        &[
            Example {
                title: "push item",
                source: r#"push(["foo"], "bar")"#,
                result: Ok(r#"["foo", "bar"]"#),
            },
            Example {
                title: "empty array",
                source: r#"push([], "bar")"#,
                result: Ok(r#"["bar"]"#),
            },
        ]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Compiled {
        let value = arguments.required("value");
        let item = arguments.required("item");

        Ok(Box::new(PushFn { value, item }))
    }
}

#[derive(Debug, Clone)]
struct PushFn {
    value: Box<dyn Expression>,
    item: Box<dyn Expression>,
}

impl Expression for PushFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let mut list = self.value.resolve(ctx)?.unwrap_array();
        let item = self.item.resolve(ctx)?;

        list.push(item);

        Ok(list.into())
    }

    fn type_def(&self, state: &state::Compiler) -> TypeDef {
        let item = TypeDef::new()
            .infallible()
            .array_mapped::<i32, TypeDef>(map! {
                0: self.item.type_def(state),
            });

        self.value.type_def(state).merge(item).infallible()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    test_function![
        push => Push;

        empty_array {
            args: func_args![value: value!([]), item: value!("foo")],
            want: Ok(value!(["foo"])),
            tdef: TypeDef::new().array_mapped::<i32, Kind>(map! {
                0: Kind::Bytes,
            }),
        }

        new_item {
            args: func_args![value: value!([11, false, 42.5]), item: value!("foo")],
            want: Ok(value!([11, false, 42.5, "foo"])),
            tdef: TypeDef::new().array_mapped::<i32, Kind>(map! {
                0: Kind::Integer,
                1: Kind::Boolean,
                2: Kind::Float,
                3: Kind::Bytes,
            }),
        }

        already_exists_item {
            args: func_args![value: value!([11, false, 42.5]), item: value!(42.5)],
            want: Ok(value!([11, false, 42.5, 42.5])),
            tdef: TypeDef::new().array_mapped::<i32, Kind>(map! {
                0: Kind::Integer,
                1: Kind::Boolean,
                2: Kind::Float,
                3: Kind::Float,
            }),
        }
    ];
}
