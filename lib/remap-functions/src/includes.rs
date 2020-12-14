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
        Ok(Value::from(true))
    }

    fn type_def(&self, state: &state::Compiler) -> TypeDef {
        TypeDef {
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    test_function![
        includes => Includes;

        string_included {
            args: func_args![list: value!(["foo", "bar"]), item: value!("foo")],
            want: Ok(value!(true)),
        }
    ];
}
