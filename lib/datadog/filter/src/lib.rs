#![deny(clippy::all)]
#![deny(clippy::pedantic)]
#![deny(unreachable_pub)]
#![deny(unused_allocation)]
#![deny(unused_extern_crates)]
#![deny(unused_assignments)]
#![deny(unused_comparisons)]
#![allow(clippy::module_name_repetitions)]

mod filter;
mod matcher;
pub mod regex;
mod resolver;

pub use filter::*;
pub use matcher::*;
pub use resolver::*;
