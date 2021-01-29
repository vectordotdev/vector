use remap::prelude::*;

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
                accepts: |v| matches!(v, Value::Array(_)),
                required: true,
            },
            Parameter {
                keyword: "item",
                accepts: |_| true,
                required: true,
            },
        ]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Result<Box<dyn Expression>> {
        let value = arguments.required("value")?.boxed();
        let item = arguments.required("item")?.boxed();

        Ok(Box::new(PushFn { value, item }))
    }
}

#[derive(Debug, Clone)]
struct PushFn {
    value: Box<dyn Expression>,
    item: Box<dyn Expression>,
}

impl Expression for PushFn {
    fn execute(&self, state: &mut state::Program, object: &mut dyn Object) -> Result<Value> {
        let mut list = self.value.execute(state, object)?.try_array()?;
        let item = self.item.execute(state, object)?;

        list.push(item);

        Ok(list.into())
    }

    fn type_def(&self, state: &state::Compiler) -> TypeDef {
        use value::Kind;

        let item_type = self.item.type_def(state).into_fallible(false);

        self.value
            .type_def(state)
            .fallible_unless(Kind::Array)
            .merge(item_type)
            .with_constraint(Kind::Array)
            .with_inner_type(
                self.item
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
        value_array_infallible {
            expr: |_| PushFn {
                value: array!["foo", "bar", 127, 42.5].boxed(),
                item: lit!(47).boxed(),
            },
            def: TypeDef {
                fallible: false,
                kind: Kind::Array,
                inner_type_def: Some(InnerTypeDef::Array(TypeDef::from(
                    Kind::Bytes | Kind::Float | Kind::Integer,
                ).boxed()))
            },
        }

        value_non_array_fallible {
            expr: |_| PushFn {
                value: lit!(27).boxed(),
                item: lit!("foo").boxed(),
            },
            def: TypeDef { kind: Kind::Array, fallible: true, ..Default::default() },
        }
    ];

    test_function![
        push => Push;

        empty_array {
            args: func_args![value: value!([]), item: value!("foo")],
            want: Ok(value!(["foo"])),
        }

        new_item {
            args: func_args![value: value!([11, false, 42.5]), item: value!("foo")],
            want: Ok(value!([11, false, 42.5, "foo"])),
        }

        already_exists_item {
            args: func_args![value: value!([11, false, 42.5]), item: value!(42.5)],
            want: Ok(value!([11, false, 42.5, 42.5])),
        }
    ];
}
