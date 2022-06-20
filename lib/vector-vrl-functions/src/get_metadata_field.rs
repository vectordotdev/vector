use crate::{get_metadata_key, MetadataKey};
use ::value::Value;
use vrl::prelude::*;

fn get_metadata_field(
    ctx: &mut Context,
    key: &MetadataKey,
) -> std::result::Result<Value, ExpressionError> {
    Ok(match key {
        MetadataKey::Legacy(key) => Value::from(ctx.target().get_secret(key)),
        MetadataKey::Query(query) => ctx
            .target()
            .get_metadata(query.path())?
            .unwrap_or(Value::Null),
    })
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
            kind: kind::ANY,
            required: true,
        }]
    }

    fn examples(&self) -> &'static [Example] {
        &[Example {
            title: "Get metadata",
            source: r#"get_metadata_field(.my_metadata_field)"#,
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
        Ok(Box::new(GetMetadataFieldFn { key }))
    }
}

#[derive(Debug, Clone)]
struct GetMetadataFieldFn {
    key: MetadataKey,
}

impl Expression for GetMetadataFieldFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        get_metadata_field(ctx, &self.key)
    }

    fn type_def(&self, _: (&state::LocalEnv, &state::ExternalEnv)) -> TypeDef {
        match &self.key {
            MetadataKey::Legacy(_) => TypeDef::bytes().add_null().infallible(),
            MetadataKey::Query(_query) => {
                // TODO: use metadata schema when it exists to return a better value here
                TypeDef::any().infallible()
            }
        }
    }
}
