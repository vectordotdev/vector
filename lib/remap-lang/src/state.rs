use crate::{path::Path, TypeDef, Value};
use std::collections::HashMap;

#[derive(Debug, Default)]
pub struct Program {
    variables: HashMap<String, Value>,
}

impl Program {
    pub fn variable(&self, key: impl AsRef<str>) -> Option<&Value> {
        self.variables.get(key.as_ref())
    }

    pub fn variables_mut(&mut self) -> &mut HashMap<String, Value> {
        &mut self.variables
    }
}

/// State held by the compiler as it parses the program source.
#[derive(Debug, Default)]
pub struct Compiler {
    /// The [`Constraint`] each variable is expected to have.
    ///
    /// This allows assignment operations to tell the compiler what kinds each
    /// variable will have at runtime, so that the compiler can then check the
    /// variable kinds at compile-time when a variable is called.
    variable_types: HashMap<String, TypeDef>,

    /// The [`Constraint`] each path query is expected to have.
    ///
    /// By default, the first time a path is queried, it resolves to `Any`, but
    /// when a path is used to assign a value to, we can potentially narrow down
    /// the list of values the path will resolve to.
    ///
    /// FIXME: this won't work for coalesced paths. We're either going to
    /// disallow those in assignments, which makes this easier to fix, or we're
    /// going to always return `Any` for coalesced paths. Either way, this is a
    /// known bug that we need to fix soon.
    path_query_types: HashMap<Path, TypeDef>,

    /// On request, the compiler can store its state in this field, which can
    /// later be used to revert the compiler state to the previously stored
    /// state.
    ///
    /// This is used by the parser to try and parse part of an expression, but
    /// back out of it if only part of the expression could be parsed. We still
    /// want the parser to continue parsing, and so it can swap the failed
    /// expression with a "no-op" one, but has to have a way for the compiler to
    /// forget any state it started tracking while parsing the old, defunct
    /// expression.
    track_changes: Option<Box<Self>>,
}

impl Compiler {
    pub fn variable_type(&self, key: impl AsRef<str>) -> Option<&TypeDef> {
        self.variable_types.get(key.as_ref())
    }

    pub fn variable_types_mut(&mut self) -> &mut HashMap<String, TypeDef> {
        &mut self.variable_types
    }

    pub fn path_query_type(&self, key: impl AsRef<Path>) -> Option<&TypeDef> {
        self.path_query_types.get(key.as_ref())
    }

    pub fn path_query_types_mut(&mut self) -> &mut HashMap<Path, TypeDef> {
        &mut self.path_query_types
    }

    pub fn track_changes(&mut self) {
        let variable_types = self.variable_types.clone();
        let path_query_types = self.path_query_types.clone();

        self.track_changes = Some(Box::new(Self {
            variable_types,
            path_query_types,
            track_changes: None,
        }));
    }

    pub fn revert_changes(&mut self) {
        if let Some(state) = self.track_changes.take() {
            *self = *state;
        }
    }
}
