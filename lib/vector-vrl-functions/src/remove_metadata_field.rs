use crate::{get_metadata_key, MetadataKey};
use ::value::kind::remove;
use ::value::Value;
use lookup::LookupBuf;
use vrl::prelude::state::TypeState;
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
        MetadataKey::Query(target_path) => {
            ctx.target_mut().target_remove(target_path, false)?;
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
            kind: kind::ANY,
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
        _state: &TypeState,
        ctx: &mut FunctionCompileContext,
        mut arguments: ArgumentList,
    ) -> Compiled {
        let key = get_metadata_key(&mut arguments)?;

        if let MetadataKey::Query(query) = &key {
            if ctx.is_read_only_path(query) {
                return Err(vrl::function::Error::ReadOnlyMutation {
                    context: format!("{} is read-only, and cannot be removed", query),
                }
                .into());
            }
        }

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

    fn type_info(&self, state: &TypeState) -> TypeInfo {
        let mut state = state.clone();

        if let MetadataKey::Query(query) = &self.key {
            let mut new_kind = state.external.metadata_kind().clone();

            let result = new_kind.remove_at_path(
                &LookupBuf::from(query.path.clone()).to_lookup(),
                remove::Strategy {
                    coalesced_path: remove::CoalescedPath::Reject,
                },
            );

            match result {
                Ok(_) => state.external.update_metadata(new_kind),
                Err(_) => {
                    // This isn't ideal, but "remove_at_path" doesn't support
                    // the path used, so no assumptions can be made about the resulting type
                    // see: https://github.com/vectordotdev/vector/issues/13460
                    state.external.update_metadata(Kind::any())
                }
            }
        }

        TypeInfo::new(state, TypeDef::null())
    }
}
