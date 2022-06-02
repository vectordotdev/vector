pub mod get_metadata_field;
pub mod get_secret;
pub mod remove_metadata_field;
pub mod remove_secret;
pub mod set_metadata_field;
pub mod set_secret;
pub mod set_semantic_meaning;

use ::value::Value;
use lookup::{Lookup, LookupBuf, SegmentBuf};
use vrl::prelude::expression::Query;
use vrl::prelude::*;

pub(crate) fn legacy_keys() -> Vec<Value> {
    vec![value!("datadog_api_key"), value!("splunk_hec_token")]
}

#[derive(Clone, Debug)]
pub enum MetadataKey {
    Legacy(String),
    Query(Query),
}

pub const LEGACY_METADATA_KEYS: [&str; 2] = ["datadog_api_key", "splunk_hec_token"];

pub fn is_legacy_metadata_path(path: &LookupBuf) -> bool {
    if path.segments.len() != 1 {
        return false;
    }
    if let Some(SegmentBuf::Field(field)) = path.segments.front() {
        if LEGACY_METADATA_KEYS.contains(&field.name.as_str()) {
            return true;
        }
    }
    false
}

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

fn compile_path_arg(path: &str) -> std::result::Result<LookupBuf, Box<dyn DiagnosticMessage>> {
    match Lookup::from_str(path) {
        Ok(lookup) => Ok(lookup.into()),
        Err(_) => Err(vrl::function::Error::InvalidArgument {
            keyword: "key",
            value: Value::Bytes(Bytes::from(path.as_bytes().to_vec())),
            error: "Invalid path",
        }
        .into()),
    }
}
