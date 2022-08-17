use ::value::Value;
use vrl::prelude::expression::FunctionExpression;
use vrl::prelude::*;

fn append(value: Value, items: Value) -> Resolved {
    let mut value = value.try_array()?;
    let mut items = items.try_array()?;
    value.append(&mut items);
    Ok(value.into())
}

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
        _state: &state::TypeState,
        _ctx: &mut FunctionCompileContext,
        mut arguments: ArgumentList,
    ) -> Compiled {
        let value = arguments.required("value");
        let items = arguments.required("items");

        Ok(AppendFn { value, items }.as_expr())
    }
}

#[derive(Debug, Clone)]
struct AppendFn {
    value: Box<dyn Expression>,
    items: Box<dyn Expression>,
}

impl FunctionExpression for AppendFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let value = self.value.resolve(ctx)?;
        let items = self.items.resolve(ctx)?;

        append(value, items)
    }

    fn type_def(&self, state: &state::TypeState) -> TypeDef {
        let mut self_value = self.value.type_def(state).restrict_array();
        let items = self.items.type_def(state).restrict_array();

        let self_array = self_value.as_array_mut().expect("must be an array");
        let items_array = items.as_array().expect("must be an array");

        if let Some(exact_len) = self_array.exact_length() {
            // The exact array length is known.
            for (i, i_kind) in items_array.known() {
                self_array
                    .known_mut()
                    .insert((i.to_usize() + exact_len).into(), i_kind.clone());
            }

            // "value" can't have an unknown, so they new unknown is just that of "items".
            self_array.set_unknown(items_array.unknown_kind());
        } else {
            // We don't know where the items will be inserted, so the union of all items will be added to the unknown.
            self_array.set_unknown(self_array.unknown_kind().union(items_array.reduced_kind()));
        }

        self_value.infallible()
    }
}

#[cfg(test)]
mod tests {
    use vector_common::btreemap;

    use super::*;

    test_function![
        append => Append;

        both_arrays_empty {
            args: func_args![value: value!([]), items: value!([])],
            want: Ok(value!([])),
            tdef: TypeDef::array(Collection::empty()),
        }

        one_array_empty {
            args: func_args![value: value!([]), items: value!([1, 2, 3])],
            want: Ok(value!([1, 2, 3])),
            tdef: TypeDef::array(btreemap! {
                Index::from(0) => Kind::integer(),
                Index::from(1) => Kind::integer(),
                Index::from(2) => Kind::integer(),
            }),
        }

        neither_array_empty {
            args: func_args![value: value!([1, 2, 3]), items: value!([4, 5, 6])],
            want: Ok(value!([1, 2, 3, 4, 5, 6])),
            tdef: TypeDef::array(btreemap! {
                Index::from(0) => Kind::integer(),
                Index::from(1) => Kind::integer(),
                Index::from(2) => Kind::integer(),
                Index::from(3) => Kind::integer(),
                Index::from(4) => Kind::integer(),
                Index::from(5) => Kind::integer(),
            }),
        }

        mixed_array_types {
            args: func_args![value: value!([1, 2, 3]), items: value!([true, 5.0, "bar"])],
            want: Ok(value!([1, 2, 3, true, 5.0, "bar"])),
            tdef: TypeDef::array(btreemap! {
                Index::from(0) => Kind::integer(),
                Index::from(1) => Kind::integer(),
                Index::from(2) => Kind::integer(),
                Index::from(3) => Kind::boolean(),
                Index::from(4) => Kind::float(),
                Index::from(5) => Kind::bytes(),
            }),
        }
    ];
}
