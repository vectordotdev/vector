pub mod prelude;
mod runtime;

pub use compiler::{
    function, state, type_def::Index, value, Context, Expression, Function, Program, Target, Value,
};
pub use diagnostic;
pub use runtime::{Runtime, RuntimeResult, Terminate};
use std::any::Any;

/// Compile a given source into the final [`Program`].
pub fn compile(
    source: &str,
    fns: &[Box<dyn Function>],
    external_context: Vec<Box<dyn Any>>,
) -> compiler::Result {
    let mut state = state::Compiler::new();
    state.add_external_context(external_context);

    compile_with_state(source, fns, &mut state)
}

pub fn compile_with_state(
    source: &str,
    fns: &[Box<dyn Function>],
    state: &mut state::Compiler,
) -> compiler::Result {
    let ast = parser::parse(source).map_err(|err| vec![Box::new(err) as _])?;

    compiler::compile_with_state(ast, fns, state)
}
