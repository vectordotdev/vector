use crate::{compile_path_arg, is_legacy_metadata_path};
use ::value::Value;
use lookup::LookupBuf;
use std::collections::VecDeque;
use vrl::prelude::*;

fn set_metadata_field(
    ctx: &mut Context,
    path: &LookupBuf,
    value: Value,
) -> std::result::Result<Value, ExpressionError> {
    ctx.target_mut().set_metadata(path, value)?;
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
                kind: kind::ANY,
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
        state: (&mut state::LocalEnv, &mut state::ExternalEnv),
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
        let path = compile_path_arg(key.as_ref())?;
        let value = arguments.required_expr("value");

        // backwards compat until schemas are supported for metadata. Make sure the "legacy"
        // fields stay as string or null
        if is_legacy_metadata_path(&path) {
            let type_def = value.type_def((&state.0, &state.1));
            if !TypeDef::bytes().add_null().is_superset(&type_def) {
                return Err(vrl::function::Error::UnexpectedExpression {
                    keyword: "key",
                    expected: "string or null",
                    expr: value,
                }
                .into());
            }
        }
        if path.len() > 1 {
            if is_legacy_metadata_path(&LookupBuf::from(VecDeque::from([path
                .segments
                .front()
                .unwrap()
                .clone()])))
            {
                return Err(vrl::function::Error::InvalidArgument {
                    keyword: "key",
                    value: Value::Bytes(Bytes::from(key.as_bytes().to_vec())),
                    error: "Cannot write to this path.",
                }
                .into());
            }
        }

        Ok(Box::new(SetMetadataFieldFn {
            path,
            value: Box::new(value),
        }))
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
        let value = args.required("value");
        let path = args
            .required_any("key")
            .downcast_ref::<LookupBuf>()
            .unwrap();

        set_metadata_field(ctx, path, value)
    }
}

#[derive(Debug, Clone)]
struct SetMetadataFieldFn {
    path: LookupBuf,
    value: Box<dyn Expression>,
}

impl Expression for SetMetadataFieldFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let value = self.value.resolve(ctx)?;
        set_metadata_field(ctx, &self.path, value)
    }

    fn type_def(&self, _: (&state::LocalEnv, &state::ExternalEnv)) -> TypeDef {
        TypeDef::null().infallible()
    }
}
