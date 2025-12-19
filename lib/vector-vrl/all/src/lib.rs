//! Central location for all VRL functions used in Vector.

/// Returns all VRL functions available in Vector.
pub fn all_vrl_functions() -> Vec<Box<dyn vrl::compiler::Function>> {
    let functions = vrl::stdlib::all()
        .into_iter()
        .chain(vector_vrl_functions::all())
        .chain(enrichment::vrl_functions());

    #[cfg(feature = "dnstap")]
    let functions = functions.chain(dnstap_parser::vrl_functions());

    functions.collect()
}
