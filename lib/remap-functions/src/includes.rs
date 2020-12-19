use remap::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct Includes;

impl Function for Includes {
    fn identifier(&self) -> &'static str {
        "includes"
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

        Ok(Box::new(IncludesFn { value, item }))
    }
}

#[derive(Debug, Clone)]
struct IncludesFn {
    value: Box<dyn Expression>,
    item: Box<dyn Expression>,
}

impl Expression for IncludesFn {
    fn execute(&self, state: &mut state::Program, object: &mut dyn Object) -> Result<Value> {
        let list = self.value.execute(state, object)?.try_array()?;
        let item = self.item.execute(state, object)?;

        let included = list.iter().any(|i| i == &item);

        Ok(included.into())
    }

    fn type_def(&self, state: &state::Compiler) -> TypeDef {
        use value::Kind;

        self.value
            .type_def(state)
            .fallible_unless(Kind::Array)
            .merge(self.item.type_def(state))
            .with_constraint(Kind::Boolean)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use value::Kind;

    test_type_def![
        value_non_empty_array_infallible {
            expr: |_| IncludesFn {
                value: Array::from(vec!["foo", "bar", "baz"]).boxed(),
                item: Literal::from("foo").boxed(),
            },
            def: TypeDef { kind: Kind::Boolean, ..Default::default() },
        }

        value_not_an_array_fallible {
            expr: |_| IncludesFn {
                value: Literal::from("foo").boxed(), // Must be an array, hence fallible
                item: Literal::from("foo").boxed(),
            },
            def: TypeDef { fallible: true, kind: Kind::Boolean, ..Default::default() },
        }
    ];

    test_function![
        includes => Includes;

        empty_not_included {
            args: func_args![value: value!([]), item: value!("foo")],
            want: Ok(value!(false)),
        }

        string_included {
            args: func_args![value: value!(["foo", "bar"]), item: value!("foo")],
            want: Ok(value!(true)),
        }

        string_not_included {
            args: func_args![value: value!(["foo", "bar"]), item: value!("baz")],
            want: Ok(value!(false)),
        }

        bool_included {
            args: func_args![value: value!([true, false]), item: value!(true)],
            want: Ok(value!(true)),
        }

        bool_not_included {
            args: func_args![value: value!([true, true]), item: value!(false)],
            want: Ok(value!(false)),
        }

        integer_included {
            args: func_args![value: value!([1, 2, 3, 4, 5]), item: value!(5)],
            want: Ok(value!(true)),
        }

        integer_not_included {
            args: func_args![value: value!([1, 2, 3, 4, 6]), item: value!(5)],
            want: Ok(value!(false)),
        }

        float_included {
            args: func_args![value: value!([0.5, 12.1, 13.075]), item: value!(13.075)],
            want: Ok(value!(true)),
        }

        float_not_included {
            args: func_args![value: value!([0.5, 12.1, 13.075]), item: value!(471.0)],
            want: Ok(value!(false)),
        }

        array_included {
            args: func_args![value: value!([[1,2,3], [4,5,6]]), item: value!([1,2,3])],
            want: Ok(value!(true)),
        }

        array_not_included {
            args: func_args![value: value!([[1,2,3], [4,5,6]]), item: value!([1,2,4])],
            want: Ok(value!(false)),
        }

        mixed_included_string {
            args: func_args![value: value!(["foo", 1, true, [1,2,3]]), item: value!("foo")],
            want: Ok(value!(true)),
        }

        mixed_not_included_string {
            args: func_args![value: value!(["bar", 1, true, [1,2,3]]), item: value!("foo")],
            want: Ok(value!(false)),
        }

        mixed_included_bool {
            args: func_args![value: value!(["foo", 1, true, [1,2,3]]), item: value!(true)],
            want: Ok(value!(true)),
        }

        mixed_not_included_bool {
            args: func_args![value: value!(["foo", 1, true, [1,2,3]]), item: value!(false)],
            want: Ok(value!(false)),
        }
    ];
}
