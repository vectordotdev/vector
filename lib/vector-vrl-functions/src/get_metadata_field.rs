use ::value::Value;
use lookup::{Look, Lookup, LookupBuf};
use vrl::prelude::*;

fn get_metadata_field(
    ctx: &mut Context,
    path: &LookupBuf,
) -> std::result::Result<Value, ExpressionError> {
    let value = ctx.target().get_metadata(path)?.unwrap_or(Value::Null);

    Ok(value)
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
            .required_literal("key")?
            .to_value()
            .as_bytes()
            .cloned()
            .expect("key not bytes");
        let key = String::from_utf8_lossy(key.as_ref());

        // TODO: fix error handling
        let lookup = Lookup::from_str(key.as_ref()).unwrap().into();

        /*
        return Err(vrl::function::Error::InvalidArgument {
                    keyword: "algorithm",
                    value: algorithm,
                    error: "Invalid algorithm",
                }
                .into());

         */

        Ok(Box::new(GetMetadataFieldFn { path: lookup }))
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
                //TODO: fix error handling
                let lookup: LookupBuf = Lookup::from_str(&key).unwrap().into();
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
        get_metadata_field(ctx, path)
    }
}

#[derive(Debug, Clone)]
struct GetMetadataFieldFn {
    path: LookupBuf,
}

impl Expression for GetMetadataFieldFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        get_metadata_field(ctx, &self.path)
    }

    fn type_def(&self, _: (&state::LocalEnv, &state::ExternalEnv)) -> TypeDef {
        // TODO: special case existing keys to a String for backwards compat
        // TODO: use metadata schema when it exists to return a better value here
        TypeDef::bytes().add_null().infallible()
    }
}
