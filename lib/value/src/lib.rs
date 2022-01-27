//! The `value` crate contains types shared across Vector libraries to support it's use of `Value`
//! and the closely linked `Kind` in support of progressive type checking.

#![deny(
    clippy::all,
    clippy::pedantic,
    future_incompatible,
    // missing_docs,
    nonstandard_style,
    rust_2018_compatibility,
    rust_2018_idioms,
    rust_2021_compatibility,
    rustdoc::bare_urls,
    rustdoc::broken_intra_doc_links,
    rustdoc::invalid_codeblock_attributes,
    rustdoc::invalid_rust_codeblocks,
    rustdoc::missing_crate_level_docs,
    rustdoc::private_doc_tests,
    rustdoc::private_intra_doc_links,
    unused
)]

pub mod kind;
mod value;

pub use kind::Kind;
pub use value::Value;
