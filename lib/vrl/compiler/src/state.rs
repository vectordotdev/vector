use crate::enrichment;
use crate::expression::assignment;
use crate::{parser::ast::Ident, TypeDef, Value};
use std::collections::HashMap;

/// The state held by the compiler.
///
/// This state allows the compiler to track certain invariants during
/// compilation, which in turn drives our progressive type checking system.
#[derive(Clone, Default)]
pub struct Compiler {
    // stored external target type definition
    target: Option<assignment::Details>,

    // stored internal variable type definitions
    variables: HashMap<Ident, assignment::Details>,

    enrichment_tables: Option<Box<dyn enrichment::TableSetup>>,

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
    /// Creates a new compiler that starts with an initial given typedef.
    pub fn new_with_type_def(type_def: TypeDef) -> Self {
        Self {
            target: Some(assignment::Details {
                type_def,
                value: None,
            }),
            variables: HashMap::new(),
            enrichment_tables: None,
            snapshot: None,
        }
    }

    /// Enrichment tables are added to the compiler state as an object of name => enrichment_table.
    pub fn new_with_enrichment_tables(enrichment_tables: Box<dyn enrichment::TableSetup>) -> Self {
        Self {
            enrichment_tables: Some(enrichment_tables),
            ..Default::default()
        }
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
        let enrichment_tables = self.enrichment_tables.clone();

        let snapshot = Self {
            target,
            variables,
            enrichment_tables,
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

    /// Returns the root typedef for the paths (not the variables) of the object.
    pub fn target_type_def(&self) -> Option<&TypeDef> {
        self.target.as_ref().map(|assignment| &assignment.type_def)
    }

    pub fn get_enrichment_tables(&self) -> Option<&dyn enrichment::TableSetup> {
        self.enrichment_tables.as_ref().map(|table| table.as_ref())
    }

    pub fn get_enrichment_tables_mut(&mut self) -> &mut Option<Box<dyn enrichment::TableSetup>> {
        &mut self.enrichment_tables
    }
}

/// The state used at runtime to track changes as they happen.
#[derive(Debug, Default)]
pub struct Runtime {
    /// The [`Value`] stored in each variable.
    variables: HashMap<Ident, Value>,
}

impl Runtime {
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
