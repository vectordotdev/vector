//! Central location for all VRL functions used in Vector.
//!
//! This crate provides a single source of truth for the complete set of VRL functions
//! available throughout Vector, combining:
//! - Standard VRL library functions (`vrl::stdlib::all`)
//! - Vector-specific functions (`vector_vrl_functions::all`)
//! - Enrichment table functions (`enrichment::vrl_functions`)
//! - DNS tap parsing functions (optional, with `dnstap` feature)

use std::sync::LazyLock;

static ALL_VRL_FUNCTIONS: LazyLock<Vec<Box<dyn vrl::compiler::Function>>> = LazyLock::new(|| {
    #[allow(clippy::disallowed_methods)]
    let functions = vrl::stdlib::all()
        .into_iter()
        .chain(vector_vrl_functions::all())
        .chain(enrichment::vrl_functions());

    #[cfg(feature = "dnstap")]
    let functions = functions.chain(dnstap_parser::vrl_functions());

    functions.collect()
});

/// Returns all VRL functions available in Vector.
///
/// This is initialized once on first call and cached for subsequent calls.
pub fn all_vrl_functions() -> &'static [Box<dyn vrl::compiler::Function>] {
    &ALL_VRL_FUNCTIONS
}
