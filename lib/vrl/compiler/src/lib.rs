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

pub use compiler::Compiler;
pub use core::{
    value, ExpressionError, MetadataTarget, Resolved, SecretTarget, Target, TargetValue,
    TargetValueRef,
};
use std::{fmt::Display, str::FromStr};

use ::serde::{Deserialize, Serialize};
pub use context::Context;
use diagnostic::DiagnosticList;
pub(crate) use diagnostic::Span;
pub use expression::Expression;
pub use function::{Function, Parameter};
pub use paste::paste;
pub use program::{Program, ProgramInfo};
use state::ExternalEnv;
pub use type_def::TypeDef;

pub type Result<T = (Program, DiagnosticList)> = std::result::Result<T, DiagnosticList>;

/// The choice of available runtimes.
#[derive(Deserialize, Serialize, Debug, Copy, Clone, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum VrlRuntime {
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
    pub(crate) use ::parser::{
        ast::{self, Ident, Node},
        Program,
    };
}
