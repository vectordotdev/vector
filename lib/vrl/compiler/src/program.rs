use lookup::TargetPath;

use crate::state::TypeState;
use crate::{
    expression::{Block, Resolved},
    Context, Expression,
};

#[derive(Debug, Clone)]
pub struct Program {
    /// The initial state that the program was compiled with.
    pub(crate) initial_state: TypeState,
    pub(crate) expressions: Block,
    pub(crate) info: ProgramInfo,
}

impl Program {
    /// Retrieves the state of the type system before the program runs.
    #[must_use]
    pub fn initial_type_state(&self) -> TypeState {
        self.initial_state.clone()
    }

    /// Retrieves the state of the type system after the program runs.
    #[must_use]
    pub fn final_type_state(&self) -> TypeState {
        self.expressions.type_info(&self.initial_state).state
    }

    /// Get detailed information about the program, as collected by the VRL
    /// compiler.
    #[must_use]
    pub fn info(&self) -> &ProgramInfo {
        &self.info
    }

    /// Resolve the program to its final [`Value`].
    ///
    /// # Errors
    ///
    /// Returns an error if the program resulted in a runtime error.
    pub fn resolve(&self, ctx: &mut Context) -> Resolved {
        self.expressions.resolve(ctx)
    }
}

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

    /// A list of possible queries made to the external [`Target`] at runtime.
    pub target_queries: Vec<TargetPath>,

    /// A list of possible assignments made to the external [`Target`] at
    /// runtime.
    pub target_assignments: Vec<TargetPath>,
}
