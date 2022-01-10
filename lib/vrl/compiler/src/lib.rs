mod compiler;
mod program;
mod runtime;
mod test_util;

pub mod expression;
pub mod function;
pub mod state;
pub mod type_def;

use std::any::Any;

pub use expression::Expression;
pub use function::{Function, Parameter};
pub use paste::paste;
pub use program::Program;
pub use runtime::{Runtime, RuntimeResult, Terminate};
pub(crate) use state::Compiler as State;
pub use type_def::TypeDef;
pub use vrl_core::{diagnostic::Span, value, Context, Value};

pub type Result = std::result::Result<Program, compiler::Errors>;

/// Compile a given source into the final [`Program`].
pub fn compile(
    source: &str,
    fns: &[Box<dyn Function>],
    external_context: Option<Box<dyn Any>>,
) -> Result {
    let mut state = state::Compiler::new();
    state.set_external_context(external_context);

    compile_with_state(source, fns, &mut state)
}

pub fn compile_with_state(
    source: &str,
    fns: &[Box<dyn Function>],
    state: &mut state::Compiler,
) -> Result {
    let ast = ::parser::parse(source).map_err(|err| vec![Box::new(err) as _])?;

    compile_ast_with_state(ast, fns, state)
}

/// Compile a given program [`ast`](parser::Program) into the final [`Program`].
pub fn compile_ast(ast: parser::Program, fns: &[Box<dyn Function>]) -> Result {
    let mut state = State::default();
    compile_ast_with_state(ast, fns, &mut state)
}

/// Similar to [`compile`], except that it takes a pre-generated [`State`]
/// object, allowing running multiple successive programs based on each others
/// state.
///
/// This is particularly useful in REPL-like environments in which you want to
/// resolve each individual expression, but allow successive expressions to use
/// the result of previous expressions.
pub fn compile_ast_with_state(
    ast: parser::Program,
    fns: &[Box<dyn Function>],
    state: &mut State,
) -> Result {
    compiler::Compiler::new(fns, state).compile(ast)
}

/// re-export of commonly used parser types.
pub(crate) mod parser {
    pub use ::parser::{
        ast::{self, Node},
        Program,
    };
}
