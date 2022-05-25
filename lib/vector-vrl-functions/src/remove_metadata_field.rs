use crate::compile_path_arg;
use ::value::Value;
use lookup::LookupBuf;
use vrl::prelude::*;

fn remove_metadata_field(
    ctx: &mut Context,
    path: &LookupBuf,
) -> std::result::Result<Value, ExpressionError> {
    ctx.target_mut().remove_metadata(path)?;
    Ok(Value::Null)
}

#[derive(Clone, Copy, Debug)]
pub struct RemoveMetadataField;

impl Function for RemoveMetadataField {
    fn identifier(&self) -> &'static str {
        "remove_metadata_field"
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
            title: "Removes the datadog api key",
            source: r#"remove_metadata_field("datadog_api_key")"#,
            result: Ok("null"),
        }]
    }

    fn compile(
        &self,
        _state: (&mut state::LocalEnv, &mut state::ExternalEnv),
        _ctx: &mut FunctionCompileContext,
        mut arguments: ArgumentList,
    ) -> Compiled {
        let key_bytes = arguments
            .required_literal("key")?
            .to_value()
            .as_bytes()
            .cloned()
            .expect("key not bytes");
        let key = String::from_utf8_lossy(key_bytes.as_ref());
        let path = compile_path_arg(key.as_ref())?;

        Ok(Box::new(RemoveMetadataFieldFn { path }))
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
                    .as_literal("key")?
                    .try_bytes_utf8_lossy()
                    .expect("key not bytes")
                    .to_string();
                let lookup = compile_path_arg(&key)?;
                Ok(Some(Box::new(lookup) as _))
            }
            _ => Ok(None),
        }
    }

    fn call_by_vm(&self, ctx: &mut Context, args: &mut VmArgumentList) -> Resolved {
        let path = args
            .required_any("key")
            .downcast_ref::<LookupBuf>()
            .unwrap();
        remove_metadata_field(ctx, path)
    }
}

#[derive(Debug, Clone)]
struct RemoveMetadataFieldFn {
    path: LookupBuf,
}

impl Expression for RemoveMetadataFieldFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        remove_metadata_field(ctx, &self.path)
    }

    fn type_def(&self, _: (&state::LocalEnv, &state::ExternalEnv)) -> TypeDef {
        TypeDef::null().infallible()
    }
}
