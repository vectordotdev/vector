use ::value::Value;
use vrl::prelude::*;

fn push(list: Value, item: Value) -> Resolved {
    let mut list = list.try_array()?;
    list.push(item);
    Ok(list.into())
}

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

    fn compile(
        &self,
        _state: &state::TypeState,
        _ctx: &mut FunctionCompileContext,
        arguments: ArgumentList,
    ) -> Compiled {
        let value = arguments.required("value");
        let item = arguments.required("item");

        Ok(PushFn { value, item }.as_expr())
    }
}

#[derive(Debug, Clone)]
struct PushFn {
    value: Box<dyn Expression>,
    item: Box<dyn Expression>,
}

impl FunctionExpression for PushFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let list = self.value.resolve(ctx)?;
        let item = self.item.resolve(ctx)?;

        push(list, item)
    }

    fn type_def(&self, state: &state::TypeState) -> TypeDef {
        let item = self.item.type_def(state).kind().clone().upgrade_undefined();
        let mut typedef = self.value.type_def(state).restrict_array();

        let array = typedef.as_array_mut().expect("must be an array");

        if let Some(exact_len) = array.exact_length() {
            // The exact array length is known, so just add the item to the correct index.
            array.known_mut().insert(exact_len.into(), item);
        } else {
            // We don't know where the item will be inserted, so just add it to the unknown.
            array.set_unknown(array.unknown_kind().union(item));
        }

        typedef.infallible()
    }
}

#[cfg(test)]
mod tests {
    use ::value::btreemap;

    use super::*;

    test_function![
        push => Push;

        empty_array {
            args: func_args![value: value!([]), item: value!("foo")],
            want: Ok(value!(["foo"])),
            tdef: TypeDef::array(btreemap! {
                Index::from(0) => Kind::bytes(),
            }),
        }

        new_item {
            args: func_args![value: value!([11, false, 42.5]), item: value!("foo")],
            want: Ok(value!([11, false, 42.5, "foo"])),
            tdef: TypeDef::array(btreemap! {
                Index::from(0) => Kind::integer(),
                Index::from(1) => Kind::boolean(),
                Index::from(2) => Kind::float(),
                Index::from(3) => Kind::bytes(),
            }),
        }

        already_exists_item {
            args: func_args![value: value!([11, false, 42.5]), item: value!(42.5)],
            want: Ok(value!([11, false, 42.5, 42.5])),
            tdef: TypeDef::array(btreemap! {
                Index::from(0) => Kind::integer(),
                Index::from(1) => Kind::boolean(),
                Index::from(2) => Kind::float(),
                Index::from(3) => Kind::float(),
            }),
        }
    ];
}
