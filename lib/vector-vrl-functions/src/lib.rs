#![deny(warnings)]

pub mod get_metadata_field;
pub mod get_secret;
pub mod remove_metadata_field;
pub mod remove_secret;
pub mod set_metadata_field;
pub mod set_secret;
pub mod set_semantic_meaning;

use ::value::Value;
use vrl::prelude::expression::Query;
use vrl::prelude::*;

pub(crate) fn legacy_keys() -> Vec<Value> {
    LEGACY_METADATA_KEYS
        .iter()
        .map(|key| (*key).into())
        .collect()
}

#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug)]
pub enum MetadataKey {
    Legacy(String),
    Query(Query),
}

pub const LEGACY_METADATA_KEYS: [&str; 2] = ["datadog_api_key", "splunk_hec_token"];

pub fn vrl_functions() -> Vec<Box<dyn vrl::Function>> {
    vec![
        Box::new(get_metadata_field::GetMetadataField) as _,
        Box::new(remove_metadata_field::RemoveMetadataField) as _,
        Box::new(set_metadata_field::SetMetadataField) as _,
        Box::new(set_semantic_meaning::SetSemanticMeaning) as _,
        Box::new(get_secret::GetSecret) as _,
        Box::new(remove_secret::RemoveSecret) as _,
        Box::new(set_secret::SetSecret) as _,
    ]
}

fn get_metadata_key(
    arguments: &mut ArgumentList,
) -> std::result::Result<MetadataKey, Box<dyn DiagnosticMessage>> {
    let key = if let Ok(Some(query)) = arguments.optional_query("key") {
        MetadataKey::Query(query)
    } else {
        let key = arguments.required_enum("key", &legacy_keys())?;
        MetadataKey::Legacy(
            key.try_bytes_utf8_lossy()
                .expect("key not bytes")
                .to_string(),
        )
    };
    Ok(key)
}
