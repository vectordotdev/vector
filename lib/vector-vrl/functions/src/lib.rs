#![deny(warnings)]

use vrl::compiler::Function;
use vrl::path::OwnedTargetPath;

pub mod get_secret;
pub mod remove_secret;
pub mod set_secret;
pub mod set_semantic_meaning;

#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug)]
pub enum MetadataKey {
    Legacy(String),
    Query(OwnedTargetPath),
}

pub const LEGACY_METADATA_KEYS: [&str; 2] = ["datadog_api_key", "splunk_hec_token"];

pub fn all() -> Vec<Box<dyn Function>> {
    vec![
        Box::new(set_semantic_meaning::SetSemanticMeaning) as _,
        Box::new(get_secret::GetSecret) as _,
        Box::new(remove_secret::RemoveSecret) as _,
        Box::new(set_secret::SetSecret) as _,
    ]
}
