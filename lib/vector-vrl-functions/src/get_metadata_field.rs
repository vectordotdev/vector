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
        (_local, external): (&mut state::LocalEnv, &mut state::ExternalEnv),
        _ctx: &mut FunctionCompileContext,
        mut arguments: ArgumentList,
    ) -> Compiled {
        let key = get_metadata_key(&mut arguments)?;
        let kind = match &key {
            MetadataKey::Legacy(_) => Kind::bytes().or_null(),
            MetadataKey::Query(query) => external.metadata_kind().at_path(query.path()),
        };
        Ok(Box::new(GetMetadataFieldFn { key, kind }))
    }
}

#[derive(Debug, Clone)]
struct GetMetadataFieldFn {
    key: MetadataKey,
    kind: Kind,
}

impl Expression for GetMetadataFieldFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        get_metadata_field(ctx, &self.key)
    }

    fn type_def(&self, _: (&state::LocalEnv, &state::ExternalEnv)) -> TypeDef {
        TypeDef::from(self.kind.clone()).infallible()
    }
}
