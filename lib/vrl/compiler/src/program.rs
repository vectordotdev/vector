use crate::Expression;
use std::iter::IntoIterator;
use std::ops::Deref;

#[derive(Debug, Clone)]
pub struct Program(pub(crate) Vec<Box<dyn Expression>>);

impl IntoIterator for Program {
    type Item = Box<dyn Expression>;
    type IntoIter = std::vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl Deref for Program {
    type Target = [Box<dyn Expression>];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
