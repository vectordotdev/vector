#![deny(warnings)]

use vrl::compiler::Function;

mod internal_events;
pub mod parser;
pub mod schema;
mod vrl_functions;

pub fn vrl_functions() -> Vec<Box<dyn Function>> {
    vrl_functions::all()
}
