use ::value::Value;
use vrl::prelude::expression::FunctionExpression;
use vrl::prelude::*;

fn encode_base16(value: Value) -> Resolved {
    let value = value.try_bytes()?;
    Ok(base16::encode_lower(&value).into())
}

#[derive(Clone, Copy, Debug)]
pub struct EncodeBase16;

impl Function for EncodeBase16 {
    fn identifier(&self) -> &'static str {
        "encode_base16"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[Parameter {
            keyword: "value",
            kind: kind::BYTES,
            required: true,
        }]
    }

    fn compile(
        &self,
        _state: &state::TypeState,
        _ctx: &mut FunctionCompileContext,
        arguments: ArgumentList,
    ) -> Compiled {
        let value = arguments.required("value");

        Ok(EncodeBase16Fn { value }.as_expr())
    }

    fn examples(&self) -> &'static [Example] {
        &[Example {
            title: "demo string",
            source: r#"encode_base16("some string value")"#,
            result: Ok("736f6d6520737472696e672076616c7565"),
        }]
    }
}

#[derive(Clone, Debug)]
struct EncodeBase16Fn {
    value: Box<dyn Expression>,
}

impl FunctionExpression for EncodeBase16Fn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let value = self.value.resolve(ctx)?;

        encode_base16(value)
    }

    fn type_def(&self, _: &state::TypeState) -> TypeDef {
        TypeDef::bytes().infallible()
    }
}

#[cfg(test)]
mod test {
    use super::*;

    test_function![
        encode_base16 => EncodeBase16;

        with_defaults {
            args: func_args![value: value!("some+=string/value")],
            want: Ok(value!("736f6d652b3d737472696e672f76616c7565")),
            tdef: TypeDef::bytes().infallible(),
        }

    ];
}
