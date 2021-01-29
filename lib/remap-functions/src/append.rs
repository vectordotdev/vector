use remap::prelude::*;

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
                accepts: |v| matches!(v, Value::Array(_)),
                required: true,
            },
            Parameter {
                keyword: "items",
                accepts: |v| matches!(v, Value::Array(_)),
                required: true,
            },
        ]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Result<Box<dyn Expression>> {
        let value = arguments.required("value")?.boxed();
        let items = arguments.required("items")?.boxed();

        Ok(Box::new(AppendFn { value, items }))
    }
}

#[derive(Debug, Clone)]
struct AppendFn {
    value: Box<dyn Expression>,
    items: Box<dyn Expression>,
}

impl Expression for AppendFn {
    fn execute(&self, state: &mut state::Program, object: &mut dyn Object) -> Result<Value> {
        let mut value = self.value.execute(state, object)?.try_array()?;
        let mut items = self.items.execute(state, object)?.try_array()?;

        value.append(&mut items);

        Ok(value.into())
    }

    fn type_def(&self, state: &state::Compiler) -> TypeDef {
        use value::Kind;

        let items_type = self
            .items
            .type_def(state)
            .fallible_unless(Kind::Array)
            .with_inner_type(self.items.type_def(state).inner_type_def);

        self.value
            .type_def(state)
            .fallible_unless(Kind::Array)
            .merge(items_type)
            .with_constraint(Kind::Array)
            .with_inner_type(
                self.items
                    .type_def(state)
                    .merge(self.value.type_def(state))
                    .inner_type_def,
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use value::Kind;

    test_type_def![
        value_array_items_array_infallible {
            expr: |_| AppendFn {
                value: array!["foo", "bar", 142].boxed(),
                items: array!["baq", "baz", true].boxed(),
            },
            def: TypeDef {
                fallible: false,
                kind: Kind::Array,
                inner_type_def: Some(inner_type_def!([ Kind::Boolean | Kind::Bytes | Kind::Integer ]))
            },
        }

        value_non_array_fallible {
            expr: |_| AppendFn {
                value: lit!(27).boxed(),
                items: array![1, 2, 3].boxed(),
            },
            def: TypeDef {
                fallible: true,
                kind: Kind::Array,
                inner_type_def: Some(inner_type_def!([ Kind::Integer ]))
            },
        }

        items_non_array_fallible {
            expr: |_| AppendFn {
                value: array![1, 2, 3].boxed(),
                items: lit!(27).boxed(),
            },
            def: TypeDef {
                fallible: true,
                kind: Kind::Array,
                inner_type_def: Some(inner_type_def!([ Kind::Integer ]))
            },
        }
    ];

    test_function![
        append => Append;

        both_arrays_empty {
            args: func_args![value: array![], items: array![]],
            want: Ok(value!([])),
        }

        one_array_empty {
            args: func_args![value: array![], items: array![1, 2, 3]],
            want: Ok(value!([1, 2, 3])),
        }

        neither_array_empty {
            args: func_args![value: array![1, 2, 3], items: array![4, 5, 6]],
            want: Ok(value!([1, 2, 3, 4, 5, 6])),
        }
    ];
}
