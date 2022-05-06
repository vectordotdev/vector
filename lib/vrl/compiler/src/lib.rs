#![deny(clippy::all)]
#![deny(unreachable_pub)]
#![deny(unused_allocation)]
#![deny(unused_extern_crates)]
#![deny(unused_assignments)]
#![deny(unused_comparisons)]

mod compiler;
mod context;
mod program;
mod test_util;

pub mod expression;
pub mod function;
pub mod state;
pub mod type_def;
pub mod value;
pub mod vm;

pub use crate::value::Value;
use ::serde::{Deserialize, Serialize};
pub use context::Context;
pub use core::{value, ExpressionError, Resolved, Target};
use diagnostic::DiagnosticList;
pub(crate) use diagnostic::Span;
pub use expression::Expression;
pub use function::{Function, Parameter};
pub use paste::paste;
pub use program::{Program, ProgramInfo};
use state::ExternalEnv;
use std::{fmt::Display, str::FromStr};
pub use type_def::TypeDef;

pub type Result<T = (Program, DiagnosticList)> = std::result::Result<T, DiagnosticList>;

/// The choice of available runtimes.
#[derive(Deserialize, Serialize, Debug, Copy, Clone, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum VrlRuntime {
    Ast,
    Vm,
}

impl Default for VrlRuntime {
    fn default() -> Self {
        Self::Ast
    }
}

impl FromStr for VrlRuntime {
    type Err = &'static str;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "ast" => Ok(Self::Ast),
            "vm" => Ok(Self::Vm),
            _ => Err("runtime must be ast or vm."),
        }
    }
}

impl Display for VrlRuntime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                VrlRuntime::Ast => "ast",
                VrlRuntime::Vm => "vm",
            }
        )
    }
}

/// Compile a given program [`ast`](parser::Program) into the final [`Program`].
pub fn compile(ast: parser::Program, fns: &[Box<dyn Function>]) -> Result {
    let mut external = ExternalEnv::default();
    compile_with_state(ast, fns, &mut external)
}

pub fn compile_for_repl(
    ast: parser::Program,
    fns: &[Box<dyn Function>],
    local: state::LocalEnv,
    external: &mut ExternalEnv,
) -> Result<Program> {
    compiler::Compiler::new_with_local_state(fns, local)
        .compile(ast, external)
        .map(|(program, _)| program)
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
    state: &mut ExternalEnv,
) -> Result {
    compiler::Compiler::new(fns).compile(ast, state)
}

/// re-export of commonly used parser types.
pub(crate) mod parser {
    pub(crate) use ::parser::{
        ast::{self, Ident, Node},
        Program,
    };
}
