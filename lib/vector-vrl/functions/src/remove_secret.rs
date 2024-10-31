use vrl::prelude::*;

fn remove_secret(ctx: &mut Context, key: Value) -> std::result::Result<Value, ExpressionError> {
    let key_bytes = key.as_bytes().expect("argument must be a string");
    let key_str = String::from_utf8_lossy(key_bytes);
    ctx.target_mut().remove_secret(key_str.as_ref());
    Ok(Value::Null)
}

#[derive(Clone, Copy, Debug)]
pub struct RemoveSecret;

impl Function for RemoveSecret {
    fn identifier(&self) -> &'static str {
        "remove_secret"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[Parameter {
            keyword: "key",
            kind: kind::BYTES,
            required: true,
        }]
    }

    fn examples(&self) -> &'static [Example] {
        &[Example {
            title: "Remove the datadog api key",
            source: r#"remove_secret("datadog_api_key")"#,
            result: Ok("null"),
        }]
    }

    fn compile(
        &self,
        _state: &TypeState,
        _ctx: &mut FunctionCompileContext,
        arguments: ArgumentList,
    ) -> Compiled {
        let key = arguments.required("key");
        Ok(RemoveSecretFn { key }.as_expr())
    }
}

#[derive(Debug, Clone)]
struct RemoveSecretFn {
    key: Box<dyn Expression>,
}

impl FunctionExpression for RemoveSecretFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let key = self.key.resolve(ctx)?;
        remove_secret(ctx, key)
    }

    fn type_def(&self, _: &TypeState) -> TypeDef {
        TypeDef::null().infallible().impure()
    }
}
