use crate::expression::CompiledExpression;
use std::iter::IntoIterator;
use std::ops::Deref;

#[derive(Debug, Clone)]
pub struct Program {
    pub(crate) expressions: Vec<CompiledExpression>,
    pub(crate) fallible: bool,
    pub(crate) abortable: bool,
    pub(crate) fanout: bool,
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

    /// Returns whether the program can fan-out an incoming event into
    /// multiple events at runtime.
    ///
    /// This happens if the final target assignment (potentially) assigns
    /// an array of elements.
    pub fn can_fanout(&self) -> bool {
        self.fanout
    }
}

impl IntoIterator for Program {
    type Item = CompiledExpression;
    type IntoIter = std::vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        self.expressions.into_iter()
    }
}

impl Deref for Program {
    type Target = [CompiledExpression];

    fn deref(&self) -> &Self::Target {
        &self.expressions
    }
}
