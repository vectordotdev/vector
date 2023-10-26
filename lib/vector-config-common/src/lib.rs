#![deny(warnings)]

// TODO: `darling` is currently strict about accepting only matching literal types for scalar fields i.e. a `f64` field
// can only be parsed from a string or float literal, but not an integer literal... and float literals have to be in the
// form of `1000.0`, not `1000`.
//
// This means we need to use float numbers for range validation if the field it's applied to is an integer.. which is
// not great from a UX perspective.  `darling` lacks the ability to incrementally parse a field to avoid having to
// expose a custom type that gets used downstream...
//
// TODO: we should add a shorthand validator for "not empty". right now, for strings, we have to say
// `#[configurable(validation(length(min = 1)))]` to indicate the string cannot be empty, when
// something like `#[configurable(validation(not_empty)]` is a bit more self-evident, and shorter to boot

use std::sync::OnceLock;

pub mod attributes;
pub mod constants;
pub mod human_friendly;
pub mod num;
pub mod schema;
pub mod validation;

/// Generate the package name to reach `vector_config` in the output of the macros. This should be
/// `vector_lib::configurable` in all packages that can import `vector_lib` to allow for a single
/// import interface, but it needs to explicitly name `vector_config` in all packages that
/// themselves import `vector_lib`.
pub fn configurable_package_name_hack() -> proc_macro2::TokenStream {
    // `TokenStream2` does not implement `Sync`, so we can't use it directly in `OnceLock`. As such,
    // this hack needs to recreate the package name token stream each time. We can also not return a
    // string type, which could be stored in the `OnceLock`, as one of the options is a multi-token
    // value and the string will always be parsed as a single token.
    static RUNNING_IN_LIB: OnceLock<bool> = OnceLock::new();
    // We can't use `env!("CARGO_PKG_NAME")` to be able to create a `const`, as that is evaluated
    // once when this macro package is built rather than when they are evaluated. This has to be
    // evaluated in the context of the package in which the macro is being expanded.
    let running_in_lib = *RUNNING_IN_LIB.get_or_init(|| {
        let package = std::env::var("CARGO_PKG_NAME").expect("Must be built by cargo");
        package.starts_with("vector-") || package == "file-source" || package == "codecs"
    });
    if running_in_lib {
        syn::parse_quote! { ::vector_config }
    } else {
        syn::parse_quote! { ::vector_lib::configurable }
    }
}
