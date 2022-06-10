use crate::{get_metadata_key, MetadataKey};
use ::value::Value;
use vrl::prelude::*;

fn remove_metadata_field(
    ctx: &mut Context,
    key: &MetadataKey,
) -> std::result::Result<Value, ExpressionError> {
    Ok(match key {
        MetadataKey::Legacy(key) => {
            ctx.target_mut().remove_secret(key);
            Value::Null
        }
        MetadataKey::Query(query) => {
            ctx.target_mut().remove_metadata(query.path())?;
            Value::Null
        }
    })
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
        let key = get_metadata_key(&mut arguments)?;
        Ok(Box::new(RemoveMetadataFieldFn { key }))
    }
}

#[derive(Debug, Clone)]
struct RemoveMetadataFieldFn {
    key: MetadataKey,
}

impl Expression for RemoveMetadataFieldFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        remove_metadata_field(ctx, &self.key)
    }

    fn type_def(&self, _: (&state::LocalEnv, &state::ExternalEnv)) -> TypeDef {
        TypeDef::null().infallible()
    }
}
