use vrl::prelude::*;

fn get_metadata_field(ctx: &mut Context, key: &str) -> std::result::Result<Value, ExpressionError> {
    ctx.target()
        .get_metadata(key)
        .map(|value| value.unwrap_or(Value::Null))
        .map_err(Into::into)
}

#[derive(Clone, Copy, Debug)]
pub struct GetMetadataField;

impl Function for GetMetadataField {
    fn identifier(&self) -> &'static str {
        "get_metadata_field"
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
            source: r#"get_metadata_field("datadog_api_key")"#,
            result: Ok("null"),
        }]
    }

    fn compile(
        &self,
        _state: (&mut state::LocalEnv, &mut state::ExternalEnv),
        _ctx: &mut FunctionCompileContext,
        mut arguments: ArgumentList,
    ) -> Compiled {
        let key = arguments
            .required_enum("key", &super::keys())?
            .try_bytes_utf8_lossy()
            .expect("key not bytes")
            .to_string();

        Ok(Box::new(GetMetadataFieldFn { key }))
    }

    fn compile_argument(
        &self,
        _args: &[(&'static str, Option<FunctionArgument>)],
        _ctx: &mut FunctionCompileContext,
        name: &str,
        expr: Option<&expression::Expr>,
    ) -> CompiledArgument {
        match (name, expr) {
            ("key", Some(expr)) => {
                let key = expr
                    .as_enum("key", super::keys())?
                    .try_bytes_utf8_lossy()
                    .expect("key not bytes")
                    .to_string();
                Ok(Some(Box::new(key) as _))
            }
            _ => Ok(None),
        }
    }

    fn call_by_vm(&self, ctx: &mut Context, args: &mut VmArgumentList) -> Resolved {
        let key = args.required_any("key").downcast_ref::<String>().unwrap();
        get_metadata_field(ctx, key)
    }
}

#[derive(Debug, Clone)]
struct GetMetadataFieldFn {
    key: String,
}

impl Expression for GetMetadataFieldFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let key = &self.key;

        get_metadata_field(ctx, key)
    }

    fn type_def(&self, _: (&state::LocalEnv, &state::ExternalEnv)) -> TypeDef {
        TypeDef::bytes().add_null().infallible()
    }
}
