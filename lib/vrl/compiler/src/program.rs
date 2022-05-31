use lookup::LookupBuf;

use crate::{
    expression::{Block, Resolved},
    state::{ExternalEnv, LocalEnv},
    Context, Expression,
};

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
    pub fn local_env(&self) -> &LocalEnv {
        &self.expressions.local_env
    }

    /// Get detailed information about the program, as collected by the VRL
    /// compiler.
    pub fn info(&self) -> &ProgramInfo {
        &self.info
    }

    /// Resolve the program to its final [`Value`].
    pub fn resolve(&self, ctx: &mut Context) -> Resolved {
        self.expressions.resolve(ctx)
    }

    /// Compile the program down to the [`Vm`] runtime.
    pub fn compile_to_vm(
        &self,
        vm: &mut crate::vm::Vm,
        state: (&mut LocalEnv, &mut ExternalEnv),
    ) -> Result<(), String> {
        self.expressions.compile_to_vm(vm, state)
    }
}

#[derive(Debug, Clone, PartialEq)]
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

    /// A list of possible queries made to the external [`Target`] at runtime.
    pub target_queries: Vec<LookupBuf>,

    /// A list of possible assignments made to the external [`Target`] at
    /// runtime.
    pub target_assignments: Vec<LookupBuf>,
}
