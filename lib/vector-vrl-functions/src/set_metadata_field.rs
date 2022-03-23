use vrl::prelude::*;

fn set_metadata_field(
    ctx: &mut Context,
    key: &str,
    value: Value,
) -> std::result::Result<Value, ExpressionError> {
    let value = value.try_bytes_utf8_lossy()?.to_string();
    ctx.target_mut().set_metadata(key, value)?;
    Ok(Value::Null)
}

#[derive(Clone, Copy, Debug)]
pub struct SetMetadataField;

impl Function for SetMetadataField {
    fn identifier(&self) -> &'static str {
        "set_metadata_field"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "key",
                kind: kind::BYTES,
                required: true,
            },
            Parameter {
                keyword: "value",
                kind: kind::BYTES,
                required: true,
            },
        ]
    }

    fn examples(&self) -> &'static [Example] {
        &[Example {
            title: "Sets the datadog api key",
            source: r#"set_metadata_field("datadog_api_key", "abc123")"#,
            result: Ok("null"),
        }]
    }

    fn compile(
        &self,
        _state: &state::Compiler,
        _ctx: &mut FunctionCompileContext,
        mut arguments: ArgumentList,
    ) -> Compiled {
        let key = arguments
            .required_enum("key", &super::keys())?
            .try_bytes_utf8_lossy()
            .expect("key not bytes")
            .to_string();
        let value = arguments.required("value");

        Ok(Box::new(SetMetadataFieldFn { key, value }))
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
        let value = args.required("value");
        let key = args.required_any("key").downcast_ref::<String>().unwrap();

        set_metadata_field(ctx, key, value)
    }
}

#[derive(Debug, Clone)]
struct SetMetadataFieldFn {
    key: String,
    value: Box<dyn Expression>,
}

impl Expression for SetMetadataFieldFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let value = self.value.resolve(ctx)?;
        let key = &self.key;

        set_metadata_field(ctx, key, value)
    }

    fn type_def(&self, _: &state::Compiler) -> TypeDef {
        TypeDef::null().infallible()
    }
}
