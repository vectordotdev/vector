pub mod get_metadata_field;
pub mod remove_metadata_field;
pub mod set_metadata_field;
pub mod set_semantic_meaning;

use ::value::Value;
use lookup::{Lookup, LookupBuf};
use vrl::prelude::*;

pub(crate) fn keys() -> Vec<Value> {
    vec![value!("datadog_api_key"), value!("splunk_hec_token")]
}

pub fn vrl_functions() -> Vec<Box<dyn vrl::Function>> {
    vec![
        Box::new(get_metadata_field::GetMetadataField) as _,
        Box::new(remove_metadata_field::RemoveMetadataField) as _,
        Box::new(set_metadata_field::SetMetadataField) as _,
        Box::new(set_semantic_meaning::SetSemanticMeaning) as _,
    ]
}

fn compile_path_arg(path: &str) -> std::result::Result<LookupBuf, Box<dyn DiagnosticMessage>> {
    match Lookup::from_str(path.as_ref()) {
        Ok(lookup) => Ok(lookup.into()),
        Err(_) => Err(vrl::function::Error::InvalidArgument {
            keyword: "key",
            value: Value::Bytes(Bytes::from(path.as_bytes().to_vec())),
            error: "Invalid path",
        }
        .into()),
    }
}
