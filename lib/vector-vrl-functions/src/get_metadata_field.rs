use crate::{get_metadata_key, MetadataKey};
use ::value::Value;
use vrl::prelude::*;
use vrl::state::TypeState;

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
        state: &TypeState,
        _ctx: &mut FunctionCompileContext,
        mut arguments: ArgumentList,
    ) -> Compiled {
        let key = get_metadata_key(&mut arguments)?;
        let kind = match &key {
            MetadataKey::Legacy(_) => Kind::bytes().or_null(),
            MetadataKey::Query(query) => state.external.metadata_kind().at_path(query.path()),
        };
        Ok(GetMetadataFieldFn { key, kind }.as_expr())
    }
}

#[derive(Debug, Clone)]
struct GetMetadataFieldFn {
    key: MetadataKey,
    kind: Kind,
}

impl FunctionExpression for GetMetadataFieldFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        get_metadata_field(ctx, &self.key)
    }

    fn type_def(&self, _: &TypeState) -> TypeDef {
        TypeDef::from(self.kind.clone()).infallible()
    }
}
