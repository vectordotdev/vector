use crate::{ResolveKind, Value};
use std::collections::HashMap;

#[derive(Debug, Default)]
pub struct State {
    variables: HashMap<String, Value>,
}

impl State {
    pub fn variable(&self, key: impl AsRef<str>) -> Option<&Value> {
        self.variables.get(key.as_ref())
    }

    pub fn variables_mut(&mut self) -> &mut HashMap<String, Value> {
        &mut self.variables
    }
}
