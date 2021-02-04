use remap::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct STring;

impl Function for STring {
    fn identifier(&self) -> &'static str {
        "string"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[Parameter {
            keyword: "value",
            kind: kind::ANY,
            required: true,
        }]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Compiled {
        let value = arguments.required("value")?;

        Ok(Box::new(StringFn { value }))
    }
}

#[derive(Debug)]
struct StringFn {
    value: Box<dyn Expression>,
}

impl Expression for StringFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        match self.value.resolve(ctx)? {
            v @ Value::Bytes(_) => Ok(v),
            v => Err(format!(r#"expected "string", got {}"#, v.kind()).into()),
        }
    }

    fn type_def(&self, state: &state::Compiler) -> TypeDef {
        self.value
            .type_def(state)
            .fallible_unless(Kind::Bytes)
            .bytes()
    }
}
