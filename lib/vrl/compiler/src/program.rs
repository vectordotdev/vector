use std::{iter::IntoIterator, ops::Deref};

use crate::Expression;

#[derive(Debug, Clone)]
pub struct Program {
    pub(crate) expressions: Vec<Box<dyn Expression>>,
    pub(crate) fallible: bool,
    pub(crate) abortable: bool,
}

impl Program {
    /// Returns whether the compiled program can fail at runtime.
    ///
    /// A program can only fail at runtime if the fallible-function-call
    /// (`foo!()`) is used within the source.
    pub fn can_fail(&self) -> bool {
        self.fallible
    }

    /// Returns whether the compiled program can be aborted at runtime.
    ///
    /// A program can only abort at runtime if there's an explicit `abort`
    /// statement in the source.
    pub fn can_abort(&self) -> bool {
        self.abortable
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
