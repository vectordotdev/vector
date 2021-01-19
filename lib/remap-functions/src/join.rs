use remap::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct Join;

impl Function for Join {
    fn identifier(&self) -> &'static str {
        "join"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "value",
                accepts: |v| matches!(v, Value::Array(_)),
                required: true,
            },
            Parameter {
                keyword: "separator",
                accepts: |v| matches!(v, Value::Bytes(_)),
                required: false,
            },
        ]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Result<Box<dyn Expression>> {
        let value = arguments.required("value")?.boxed();
        let separator = arguments.optional("separator").map(Expr::boxed);

        Ok(Box::new(JoinFn { value, separator }))
    }
}

#[derive(Clone, Debug)]
struct JoinFn {
    value: Box<dyn Expression>,
    separator: Option<Box<dyn Expression>>,
}

impl Expression for JoinFn {
    fn execute(&self, state: &mut state::Program, object: &mut dyn Object) -> Result<Value> {
        Ok(Value::from("ok"))
    }

    fn type_def(&self, state: &state::Compiler) -> TypeDef {
        use value::Kind;

        let separator_type_def = self
            .separator
            .as_ref()
            .map(|sep| sep.type_def(state).fallible_unless(Kind::Bytes));

        self
            .value
            .type_def(state)
            .merge_optional(separator_type_def)
            .fallible_unless(Kind::Array)
            .with_constraint(Kind::Bytes)

    }
}
