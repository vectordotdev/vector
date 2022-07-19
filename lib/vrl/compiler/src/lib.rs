#![deny(
    warnings,
    clippy::all,
    clippy::pedantic,
    unreachable_pub,
    unused_allocation,
    unused_extern_crates,
    unused_assignments,
    unused_comparisons
)]
#![allow(
    clippy::cast_possible_truncation, // allowed in initial deny commit
    clippy::cast_possible_wrap, // allowed in initial deny commit
    clippy::cast_precision_loss, // allowed in initial deny commit
    clippy::cast_sign_loss, // allowed in initial deny commit
    clippy::if_not_else, // allowed in initial deny commit
    clippy::let_underscore_drop, // allowed in initial deny commit
    clippy::match_bool, // allowed in initial deny commit
    clippy::match_same_arms, // allowed in initial deny commit
    clippy::match_wild_err_arm, // allowed in initial deny commit
    clippy::missing_errors_doc, // allowed in initial deny commit
    clippy::missing_panics_doc, // allowed in initial deny commit
    clippy::module_name_repetitions, // allowed in initial deny commit
    clippy::needless_pass_by_value, // allowed in initial deny commit
    clippy::return_self_not_must_use, // allowed in initial deny commit
    clippy::semicolon_if_nothing_returned,  // allowed in initial deny commit
    clippy::similar_names, // allowed in initial deny commit
    clippy::too_many_lines, // allowed in initial deny commit
)]

mod compiler;
mod context;
mod program;
mod test_util;

pub mod expression;
pub mod function;
pub mod state;
pub mod type_def;
pub mod value;

pub use compiler::Compiler;
pub use core::{
    value, ExpressionError, MetadataTarget, Resolved, SecretTarget, Target, TargetValue,
    TargetValueRef,
};
use std::{fmt::Display, str::FromStr};

pub use context::Context;
use diagnostic::DiagnosticList;
pub(crate) use diagnostic::Span;
pub use expression::Expression;
pub use function::{Function, Parameter};
pub use paste::paste;
pub use program::{Program, ProgramInfo};
pub use type_def::TypeDef;
use vector_config::configurable_component;

pub type Result<T = (Program, DiagnosticList)> = std::result::Result<T, DiagnosticList>;

/// Available VRL runtimes.
#[configurable_component]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum VrlRuntime {
    /// Tree-walking runtime.
    ///
    /// This is the only, and default, runtime.
    Ast,
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
            _ => Err("runtime must be ast."),
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
            }
        )
    }
}

/// re-export of commonly used parser types.
pub(crate) mod parser {
    pub(crate) use ::parser::ast::{self, Ident, Node};
}
