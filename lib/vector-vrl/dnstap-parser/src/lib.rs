#![warn(clippy::pedantic)]
#![deny(warnings)]

use vrl::compiler::Function;

mod internal_events;
pub mod parser;
pub mod schema;
mod vrl_functions;

#[must_use]
pub fn vrl_functions() -> Vec<Box<dyn Function>> {
    vrl_functions::all()
}
