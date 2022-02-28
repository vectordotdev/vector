use anymap::AnyMap;
use std::collections::HashMap;

use value::Kind;

use crate::{expression::assignment, parser::ast::Ident, Value};

/// The state held by the compiler.
///
/// This state allows the compiler to track certain invariants during
/// compilation, which in turn drives our progressive type checking system.
#[derive(Debug)]
pub struct Compiler {
    /// Stored external target type definitions.
    //
    // TODO(Jean): Change this to point to a `value::Collection<Field>` type, to ensure the target
    // is always an object. Although we probably also need to support arrays, in which case we
    // should wrap it in an enum to also support `value::Collection<Index>`.
    target: Option<assignment::Details>,

    /// Stored internal variable type definitions.
    variables: HashMap<Ident, assignment::Details>,

    /// Context passed between the client program and a VRL function.
    external_context: AnyMap,

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

impl Default for Compiler {
    fn default() -> Self {
        Self {
            external_context: AnyMap::new(),
            target: None,
            variables: HashMap::default(),
            snapshot: None,
        }
    }
}

impl Compiler {
    pub fn new() -> Self {
        Default::default()
    }

    /// Creates a new compiler that starts with an initial given [`Kind`].
    pub fn new_with_kind(kind: Kind) -> Self {
        Self {
            target: Some(assignment::Details {
                type_def: kind.into(),
                value: None,
            }),
            ..Default::default()
        }
    }

    /// Get the kind information of the program target (e.g. the type accessed through `.`).
    pub fn target_kind(&self) -> Option<&Kind> {
        self.target()
            .as_ref()
            .map(|details| details.type_def.kind())
    }

    pub(crate) fn variable_idents(&self) -> impl Iterator<Item = &Ident> + '_ {
        self.variables.keys()
    }

    pub(crate) fn variable(&self, ident: &Ident) -> Option<&assignment::Details> {
        self.variables.get(ident)
    }

    pub(crate) fn insert_variable(&mut self, ident: Ident, details: assignment::Details) {
        self.variables.insert(ident, details);
    }

    pub(crate) fn target(&self) -> Option<&assignment::Details> {
        self.target.as_ref()
    }

    pub(crate) fn update_target(&mut self, details: assignment::Details) {
        self.target = Some(details);
    }

    /// Take a snapshot of the current state of the compiler.
    ///
    /// This overwrites any existing snapshot currently stored.
    pub(crate) fn snapshot(&mut self) {
        let target = self.target.clone();
        let variables = self.variables.clone();

        let snapshot = Self {
            target,
            variables,
            external_context: AnyMap::new(),
            snapshot: None,
        };

        self.snapshot = Some(Box::new(snapshot));
    }

    /// Roll back the compiler state to a previously stored snapshot.
    pub(crate) fn rollback(&mut self) {
        if let Some(snapshot) = self.snapshot.take() {
            let external_context = self.swap_external_context(AnyMap::new());
            *self = *snapshot;
            self.external_context = external_context;
        }
    }

    /// Sets the external context data for VRL functions to use.
    pub fn set_external_context<T: 'static>(&mut self, data: T) {
        self.external_context.insert::<T>(data);
    }

    /// Retrieves the first data of the required type from the external context.
    #[must_use]
    pub fn get_external_context<T: 'static>(&self) -> Option<&T> {
        self.external_context.get::<T>()
    }

    /// Retrieves a mutable reference to the first data of the required type from
    /// the external context.
    #[must_use]
    pub fn get_external_context_mut<T: 'static>(&mut self) -> Option<&mut T> {
        self.external_context.get_mut::<T>()
    }

    /// Swap the existing external contexts with new ones, returning the old ones.
    #[must_use]
    pub(crate) fn swap_external_context(&mut self, ctx: AnyMap) -> AnyMap {
        std::mem::replace(&mut self.external_context, ctx)
    }
}

/// The state used at runtime to track changes as they happen.
#[derive(Debug, Default)]
pub struct Runtime {
    /// The [`Value`] stored in each variable.
    variables: HashMap<Ident, Value>,
}

impl Runtime {
    pub fn is_empty(&self) -> bool {
        self.variables.is_empty()
    }

    pub fn clear(&mut self) {
        self.variables.clear();
    }

    pub fn variable(&self, ident: &Ident) -> Option<&Value> {
        self.variables.get(ident)
    }

    pub fn variable_mut(&mut self, ident: &Ident) -> Option<&mut Value> {
        self.variables.get_mut(ident)
    }

    pub(crate) fn insert_variable(&mut self, ident: Ident, value: Value) {
        self.variables.insert(ident, value);
    }
}
