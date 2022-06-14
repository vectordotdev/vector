#![deny(clippy::all)]
#![deny(unused_allocation)]
#![deny(unused_extern_crates)]
#![deny(unused_assignments)]
#![deny(unused_comparisons)]

mod ast;
#[doc(hidden)]
pub mod filters; // TODO Must be exposed for criterion. Perhaps we should pass a feature? Yuck.
mod grok;
mod grok_filter;
mod lexer;
mod matchers;
pub mod parse_grok;
mod parse_grok_pattern;
pub mod parse_grok_rules;
