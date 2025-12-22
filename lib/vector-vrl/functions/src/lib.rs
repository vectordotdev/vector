//! Central location for all VRL functions used in Vector.
//!
//! This crate provides a single source of truth for the complete set of VRL functions
//! available throughout Vector, combining:
//! - Standard VRL library functions (`vrl::stdlib::all`)
//! - Vector-specific functions (`vector_vrl::secret_functions`)
//! - Enrichment table functions (`enrichment::vrl_functions`)
//! - DNS tap parsing functions (optional, with `dnstap` feature)

#![deny(warnings)]

use vrl::{compiler::Function, path::OwnedTargetPath};

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

/// Returns Vector-specific secret functions.
pub fn secret_functions() -> Vec<Box<dyn Function>> {
    vec![
        Box::new(set_semantic_meaning::SetSemanticMeaning) as _,
        Box::new(get_secret::GetSecret) as _,
        Box::new(remove_secret::RemoveSecret) as _,
        Box::new(set_secret::SetSecret) as _,
    ]
}

/// Returns all VRL functions available in Vector.
#[allow(clippy::disallowed_methods)]
pub fn all() -> Vec<Box<dyn Function>> {
    let functions = vrl::stdlib::all()
        .into_iter()
        .chain(secret_functions())
        .chain(enrichment::vrl_functions());

    #[cfg(feature = "dnstap")]
    let functions = functions.chain(dnstap_parser::vrl_functions());

    functions.collect()
}
