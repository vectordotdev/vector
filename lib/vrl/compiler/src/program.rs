use std::{iter::IntoIterator, ops::Deref};

use lookup::LookupBuf;

use crate::{state::LocalEnv, Expression};

#[derive(Debug, Clone)]
pub struct Program {
    pub(crate) expressions: Vec<Box<dyn Expression>>,
    pub(crate) info: ProgramInfo,

    /// A copy of the local environment at program compilation.
    ///
    /// Can be used to instantiate a new program with the same local state as
    /// the previous program.
    ///
    /// Specifically, this is used by the VRL REPL to incrementally compile
    /// a program as each line is compiled.
    pub(crate) local_env: LocalEnv,
}

impl Program {
    /// Get a reference to the final local environment of the compiler that
    /// compiled the current program.
    pub fn local_env(&self) -> &LocalEnv {
        &self.local_env
    }

    /// Get detailed information about the program, as collected by the VRL
    /// compiler.
    pub fn info(&self) -> &ProgramInfo {
        &self.info
    }
}

impl IntoIterator for Program {
    type Item = Box<dyn Expression>;
    type IntoIter = std::vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        self.expressions.into_iter()
    }
}

impl Deref for Program {
    type Target = [Box<dyn Expression>];

    fn deref(&self) -> &Self::Target {
        &self.expressions
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
