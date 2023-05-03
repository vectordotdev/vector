use vrl::prelude::*;

fn set_secret(
    ctx: &mut Context,
    key: Value,
    secret: Value,
) -> std::result::Result<Value, ExpressionError> {
    let key_str = String::from_utf8_lossy(key.as_bytes().expect("key must be a string"));
    let secret_str = String::from_utf8_lossy(secret.as_bytes().expect("secret must be a string"));

    ctx.target_mut()
        .insert_secret(key_str.as_ref(), secret_str.as_ref());
    Ok(Value::Null)
}

#[derive(Clone, Copy, Debug)]
pub struct SetSecret;

impl Function for SetSecret {
    fn identifier(&self) -> &'static str {
        "set_secret"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "key",
                kind: kind::BYTES,
                required: true,
            },
            Parameter {
                keyword: "secret",
                kind: kind::BYTES,
                required: true,
            },
        ]
    }

    fn examples(&self) -> &'static [Example] {
        &[Example {
            title: "Set the datadog api key",
            source: r#"set_secret("datadog_api_key", "secret-value")"#,
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
        let secret = arguments.required("secret");
        Ok(SetSecretFn { key, secret }.as_expr())
    }
}

#[derive(Debug, Clone)]
struct SetSecretFn {
    key: Box<dyn Expression>,
    secret: Box<dyn Expression>,
}

impl FunctionExpression for SetSecretFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let key = self.key.resolve(ctx)?;
        let secret = self.secret.resolve(ctx)?;
        set_secret(ctx, key, secret)
    }

    fn type_def(&self, _: &TypeState) -> TypeDef {
        TypeDef::null().infallible()
    }
}
