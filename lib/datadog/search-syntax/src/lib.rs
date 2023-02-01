#![deny(warnings)]
#![deny(clippy::all)]
#![deny(unused_allocation)]
#![deny(unused_extern_crates)]
#![deny(unused_assignments)]
#![deny(unused_comparisons)]

#[macro_use]
extern crate pest_derive;

mod field;
mod grammar;
mod node;
mod parser;

pub use field::{normalize_fields, Field};
pub use node::{BooleanType, Comparison, ComparisonValue, QueryNode};
pub use parser::{parse, Error};
