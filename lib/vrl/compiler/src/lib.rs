mod compiler;
mod context;
mod program;
mod target;
mod test_util;

pub mod expression;
pub mod function;
pub mod path;
pub mod state;
pub mod type_def;
pub mod value;

pub(crate) use diagnostic::Span;
pub(crate) use state::Compiler as State;

pub use context::Context;
pub use expression::{Expression, ExpressionError, Resolved};
pub use function::{Function, Parameter};
pub use path::Path;
pub use program::Program;
pub use target::Target;
pub use type_def::TypeDef;
pub use value::Value;

pub use paste::paste;

pub type Result = std::result::Result<Program, compiler::Errors>;

/// Compile a given program [`ast`](parser::Program) into the final [`Program`].
pub fn compile(ast: parser::Program, fns: &[Box<dyn Function>]) -> Result {
    let mut state = State::default();
    compile_with_state(ast, fns, &mut state)
}

/// Similar to [`compile`], except that it takes a pre-generated [`State`]
/// object, allowing running multiple successive programs based on each others
/// state.
///
/// This is particularly useful in REPL-like environments in which you want to
/// resolve each individual expression, but allow successive expressions to use
/// the result of previous expressions.
pub fn compile_with_state(
    ast: parser::Program,
    fns: &[Box<dyn Function>],
    state: &mut State,
) -> Result {
    compiler::Compiler::new(fns, state).compile(ast)
}

/// re-export of commonly used parser types.
pub(crate) mod parser {
    pub use ::parser::ast::{self, Ident, Node};
    pub use ::parser::{Field, Path, PathSegment, Program};
}
