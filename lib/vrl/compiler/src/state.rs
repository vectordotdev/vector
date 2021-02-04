use crate::expression::assignment;
use crate::{parser::ast::Ident, TypeDef, Value};
use std::collections::HashMap;

/// The state held by the compiler.
///
/// This state allows the compiler to track certain invariants during
/// compilation, which in turn drives our progressive type checking system.
#[derive(Clone, Default)]
pub struct Compiler {
    /// Keeps track of [`Variable`](crate::expression::Variable) or
    /// [`Target`](crate::Target) assignments.
    assignments: HashMap<assignment::Target, TypeDef>,

    /// On request, the compiler can store its state in this field, which can
    /// later be used to revert the compiler state to the previously stored
    /// state.
    ///
    /// This is used by the compiler to try and parse part of an expression, but
    /// back out of it if only part of the expression could be parsed. We still
    /// want the parser to continue parsing, and so it can swap the failed
    /// expression with a "no-op" one, but has to have a way for the compiler to
    /// forget any state it started tracking while parsing the old, defunct
    /// expression.
    snapshot: Option<Box<Self>>,
}

impl Compiler {
    pub(crate) fn assignment(&self, target: &assignment::Target) -> Option<&TypeDef> {
        self.assignments.get(target)
    }

    pub(crate) fn insert_assignment(&mut self, target: assignment::Target, type_def: TypeDef) {
        self.assignments.insert(target, type_def);
    }

    /// Take a snapshot of the current state of the compiler.
    ///
    /// This overwrites any existing snapshot currently stored.
    pub(crate) fn snapshot(&mut self) {
        let assignments = self.assignments.clone();

        let snapshot = Self {
            assignments,
            snapshot: None,
        };

        self.snapshot = Some(Box::new(snapshot));
    }

    /// Roll back the compiler state to a previously stored snapshot.
    pub(crate) fn rollback(&mut self) {
        if let Some(snapshot) = self.snapshot.take() {
            *self = *snapshot;
        }
    }
}

/// The state used at runtime to track changes as they happen.
#[derive(Debug, Default)]
pub struct Runtime {
    /// The [`Value`] stored in each variable.
    variables: HashMap<Ident, Value>,
}

impl Runtime {
    pub(crate) fn variable(&self, ident: &Ident) -> Option<&Value> {
        self.variables.get(&ident)
    }

    pub(crate) fn variable_mut(&mut self, ident: &Ident) -> Option<&mut Value> {
        self.variables.get_mut(&ident)
    }

    pub(crate) fn insert_variable(&mut self, ident: Ident, value: Value) {
        self.variables.insert(ident, value);
    }
}
