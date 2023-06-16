use vrl::prelude::*;

fn get_secret(ctx: &mut Context, key: Value) -> std::result::Result<Value, ExpressionError> {
    let key_bytes = key.as_bytes().expect("argument must be a string");
    let key_str = String::from_utf8_lossy(key_bytes);
    let value = match ctx.target().get_secret(key_str.as_ref()) {
        Some(secret) => secret.into(),
        None => Value::Null,
    };
    Ok(value)
}

#[derive(Clone, Copy, Debug)]
pub struct GetSecret;

impl Function for GetSecret {
    fn identifier(&self) -> &'static str {
        "get_secret"
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
            title: "Get the datadog api key",
            source: r#"get_secret("datadog_api_key")"#,
            result: Ok("secret value"),
        }]
    }

    fn compile(
        &self,
        _state: &TypeState,
        _ctx: &mut FunctionCompileContext,
        arguments: ArgumentList,
    ) -> Compiled {
        let key = arguments.required("key");
        Ok(GetSecretFn { key }.as_expr())
    }
}

#[derive(Debug, Clone)]
struct GetSecretFn {
    key: Box<dyn Expression>,
}

impl FunctionExpression for GetSecretFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let key = self.key.resolve(ctx)?;
        get_secret(ctx, key)
    }

    fn type_def(&self, _: &TypeState) -> TypeDef {
        TypeDef::bytes().add_null().infallible()
    }
}
