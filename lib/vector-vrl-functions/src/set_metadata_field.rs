use crate::{get_metadata_key, MetadataKey};
use ::value::Value;
use vrl::prelude::*;
use vrl::state::TypeState;

fn set_metadata_field(
    ctx: &mut Context,
    key: &MetadataKey,
    value: Value,
) -> std::result::Result<Value, ExpressionError> {
    Ok(match key {
        MetadataKey::Legacy(key) => {
            let str_value = value.as_str().expect("must be a string");
            ctx.target_mut().insert_secret(key, str_value.as_ref());
            Value::Null
        }
        MetadataKey::Query(path) => {
            ctx.target_mut().target_insert(path, value)?;
            Value::Null
        }
    })
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
                kind: kind::ANY,
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
        state: &TypeState,
        ctx: &mut FunctionCompileContext,
        mut arguments: ArgumentList,
    ) -> Compiled {
        let key = get_metadata_key(&mut arguments)?;
        let value = arguments.required_expr("value");
        let value_type_def = value.type_def(state);

        if let MetadataKey::Query(target_path) = &key {
            if ctx.is_read_only_path(target_path) {
                return Err(vrl::function::Error::ReadOnlyMutation {
                    context: format!("{} is read-only, and cannot be modified", target_path),
                }
                .into());
            }
        }

        // for backwards compatibility, make sure value is a string when using legacy.
        if matches!(key, MetadataKey::Legacy(_)) && !value_type_def.is_bytes() {
            return Err(vrl::function::Error::UnexpectedExpression {
                keyword: "value",
                expected: "string",
                expr: value,
            }
            .into());
        }

        Ok(Box::new(SetMetadataFieldFn {
            key,
            value: Box::new(value),
        }))
    }
}

#[derive(Debug, Clone)]
struct SetMetadataFieldFn {
    key: MetadataKey,
    value: Box<dyn Expression>,
}

impl Expression for SetMetadataFieldFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let value = self.value.resolve(ctx)?;
        set_metadata_field(ctx, &self.key, value)
    }

    fn type_info(&self, state: &TypeState) -> TypeInfo {
        let mut state = state.clone();

        if let MetadataKey::Query(target_path) = &self.key {
            let insert_type = self.value.apply_type_info(&mut state).kind().clone();
            let mut new_type = state.external.kind(target_path.prefix);
            new_type.insert(&target_path.path, insert_type);
            state.external.update_metadata(new_type);
        }
        TypeInfo::new(state, TypeDef::null())
    }
}
