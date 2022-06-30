use lookup::LookupBuf;

use crate::{
    expression::{Block, Resolved},
    state::LocalEnv,
    Context, Expression,
};

/// A valid compiled program that can be resolved against an external
/// [`Target`](crate::Target).
#[derive(Debug, Clone)]
pub struct Program {
    pub(crate) expressions: Block,
    pub(crate) info: ProgramInfo,
}

impl Program {
    /// Get a reference to the final local environment of the compiler that
    /// compiled the current program.
    ///
    /// Can be used to instantiate a new program with the same local state as
    /// the previous program.
    ///
    /// Specifically, this is used by the VRL REPL to incrementally compile
    /// a program as each line is compiled.
    #[must_use]
    pub fn local_env(&self) -> &LocalEnv {
        &self.expressions.local_env
    }

    /// Get detailed information about the program, as collected by the VRL
    /// compiler.
    #[must_use]
    pub fn info(&self) -> &ProgramInfo {
        &self.info
    }

    /// Resolve the program to its final [`Value`](value::Value).
    ///
    /// # Errors
    ///
    /// Returns an error if the program resulted in a runtime error.
    pub fn resolve(&self, ctx: &mut Context) -> Resolved {
        self.expressions.resolve(ctx)
    }
}

/// Additional details about the compiled program.
///
/// This information is additive, it is not needed to successfully resolve
/// a compiled program, but can be used by the callee to determine how to use
/// the program at runtime.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProgramInfo {
    /// Returns whether the compiled program can fail at runtime.
    ///
    /// A program can only fail at runtime if the fallible-function-call
    /// (`foo!()`) is used within the source.
    pub fallible: bool,

    /// Returns whether the compiled program can be aborted at runtime.
    ///
    /// A program can only abort at runtime if there's an explicit `abort`
    /// statement in the source.
    pub abortable: bool,

    /// A list of possible queries made to the external
    /// [`Target`](crate::Target) at runtime.
    pub target_queries: Vec<LookupBuf>,

    /// A list of possible assignments made to the external
    /// [`Target`](crate::Target) at runtime.
    pub target_assignments: Vec<LookupBuf>,
}
