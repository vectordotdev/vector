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
                keyword: "list",
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
        let list = arguments.required("list")?.boxed();
        let item = arguments.required("item")?.boxed();

        Ok(Box::new(IncludesFn { list, item }))
    }
}

#[derive(Debug, Clone)]
struct IncludesFn {
    list: Box<dyn Expression>,
    item: Box<dyn Expression>,
}

impl Expression for IncludesFn {
    fn execute(&self, state: &mut state::Program, object: &mut dyn Object) -> Result<Value> {
        let list = self.list.execute(state, object)?.try_array()?;
        let item = self.item.execute(state, object)?;

        let included: bool = if list.is_empty() {
            false
        } else {
            list.iter().any(|i| i == &item)
        };

        Ok(Value::from(included))
    }

    fn type_def(&self, _state: &state::Compiler) -> TypeDef {
        TypeDef {
            kind: value::Kind::Boolean,
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    test_function![
        includes => Includes;

        empty_not_included {
            args: func_args![list: value!([]), item: value!("foo")],
            want: Ok(value!(false)),
        }

        string_included {
            args: func_args![list: value!(["foo", "bar"]), item: value!("foo")],
            want: Ok(value!(true)),
        }

        string_not_included {
            args: func_args![list: value!(["foo", "bar"]), item: value!("baz")],
            want: Ok(value!(false)),
        }

        bool_included {
            args: func_args![list: value!([true, false]), item: value!(true)],
            want: Ok(value!(true)),
        }

        bool_not_included {
            args: func_args![list: value!([true, true]), item: value!(false)],
            want: Ok(value!(false)),
        }

        integer_included {
            args: func_args![list: value!([1, 2, 3, 4, 5]), item: value!(5)],
            want: Ok(value!(true)),
        }

        integer_not_included {
            args: func_args![list: value!([1, 2, 3, 4, 6]), item: value!(5)],
            want: Ok(value!(false)),
        }

        mixed_included_string {
            args: func_args![list: value!(["foo", 1, true, [1,2,3]]), item: value!("foo")],
            want: Ok(value!(true)),
        }

        mixed_not_included_string {
            args: func_args![list: value!(["bar", 1, true, [1,2,3]]), item: value!("foo")],
            want: Ok(value!(false)),
        }

        mixed_included_bool {
            args: func_args![list: value!(["foo", 1, true, [1,2,3]]), item: value!(true)],
            want: Ok(value!(true)),
        }

        mixed_not_included_bool {
            args: func_args![list: value!(["foo", 1, true, [1,2,3]]), item: value!(false)],
            want: Ok(value!(false)),
        }
    ];
}
