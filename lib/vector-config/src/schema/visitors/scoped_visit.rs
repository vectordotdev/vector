use std::collections::VecDeque;

use vector_config_common::schema::{visit::Visitor, *};

/// A schema reference which can refer to either a schema definition or the root schema itself.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum SchemaReference {
    /// A defined schema.
    Definition(String),

    /// The root schema itself.
    Root,
}

impl std::fmt::Display for SchemaReference {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Definition(name) => write!(f, "{}", name),
            Self::Root => write!(f, "<root>"),
        }
    }
}

impl<'a, T> From<&'a T> for SchemaReference
where
    T: AsRef<str> + ?Sized,
{
    fn from(value: &'a T) -> Self {
        Self::Definition(value.as_ref().to_string())
    }
}

impl AsRef<str> for SchemaReference {
    fn as_ref(&self) -> &str {
        match self {
            Self::Definition(name) => name.as_str(),
            Self::Root => "<root>",
        }
    }
}

/// The schema scope stack is used to understand where in the visiting of a root schema the visitor
/// currently is. All visiting will inherently start at the root, and continue to exist in the root
/// scope until a schema reference is resolved, and the visitor visits the resolved schema. Once
/// that happens, the resolved schema scope is pushed onto the stack, such that the scope is now the
/// schema reference. This can happen multiple times as subsequent schema references are resolved.
/// Once the visitor recurses back out of the resolved schema, it is popped from the stack.
#[derive(Debug, Default)]
pub struct SchemaScopeStack {
    stack: VecDeque<SchemaReference>,
}

impl SchemaScopeStack {
    pub fn push<S: Into<SchemaReference>>(&mut self, scope: S) {
        self.stack.push_front(scope.into());
    }

    pub fn pop(&mut self) -> Option<SchemaReference> {
        self.stack.pop_front()
    }

    pub fn current(&self) -> Option<&SchemaReference> {
        self.stack.front()
    }
}

pub trait ScopedVisitor: Visitor {
    fn push_schema_scope<S: Into<SchemaReference>>(&mut self, scope: S);

    fn pop_schema_scope(&mut self);

    fn get_current_schema_scope(&self) -> &SchemaReference;
}

pub fn visit_schema_object_scoped<SV: ScopedVisitor + ?Sized>(
    sv: &mut SV,
    definitions: &mut Map<String, Schema>,
    schema: &mut SchemaObject,
) {
    if let Some(reference) = schema.reference.as_ref() {
        let schema_def_key = get_cleaned_schema_reference(reference);
        let mut referenced_schema = definitions
            .get(schema_def_key)
            .cloned()
            .expect("schema reference should exist");

        if let Schema::Object(referenced_schema) = &mut referenced_schema {
            sv.push_schema_scope(schema_def_key);

            sv.visit_schema_object(definitions, referenced_schema);

            sv.pop_schema_scope();
        }

        definitions.insert(schema_def_key.to_string(), referenced_schema);
    }

    if let Some(sub) = &mut schema.subschemas {
        visit_vec_scoped(sv, definitions, &mut sub.all_of);
        visit_vec_scoped(sv, definitions, &mut sub.any_of);
        visit_vec_scoped(sv, definitions, &mut sub.one_of);
        visit_box_scoped(sv, definitions, &mut sub.not);
        visit_box_scoped(sv, definitions, &mut sub.if_schema);
        visit_box_scoped(sv, definitions, &mut sub.then_schema);
        visit_box_scoped(sv, definitions, &mut sub.else_schema);
    }

    if let Some(arr) = &mut schema.array {
        visit_single_or_vec_scoped(sv, definitions, &mut arr.items);
        visit_box_scoped(sv, definitions, &mut arr.additional_items);
        visit_box_scoped(sv, definitions, &mut arr.contains);
    }

    if let Some(obj) = &mut schema.object {
        visit_map_values_scoped(sv, definitions, &mut obj.properties);
        visit_map_values_scoped(sv, definitions, &mut obj.pattern_properties);
        visit_box_scoped(sv, definitions, &mut obj.additional_properties);
        visit_box_scoped(sv, definitions, &mut obj.property_names);
    }
}

fn visit_box_scoped<SV: ScopedVisitor + ?Sized>(
    sv: &mut SV,
    definitions: &mut Map<String, Schema>,
    target: &mut Option<Box<Schema>>,
) {
    if let Some(s) = target {
        if let Schema::Object(s) = s.as_mut() {
            sv.visit_schema_object(definitions, s);
        }
    }
}

fn visit_vec_scoped<SV: ScopedVisitor + ?Sized>(
    sv: &mut SV,
    definitions: &mut Map<String, Schema>,
    target: &mut Option<Vec<Schema>>,
) {
    if let Some(vec) = target {
        for s in vec {
            if let Schema::Object(s) = s {
                sv.visit_schema_object(definitions, s);
            }
        }
    }
}

fn visit_map_values_scoped<SV: ScopedVisitor + ?Sized>(
    sv: &mut SV,
    definitions: &mut Map<String, Schema>,
    target: &mut Map<String, Schema>,
) {
    for s in target.values_mut() {
        if let Schema::Object(s) = s {
            sv.visit_schema_object(definitions, s);
        }
    }
}

fn visit_single_or_vec_scoped<SV: ScopedVisitor + ?Sized>(
    sv: &mut SV,
    definitions: &mut Map<String, Schema>,
    target: &mut Option<SingleOrVec<Schema>>,
) {
    match target {
        None => {}
        Some(SingleOrVec::Single(s)) => {
            if let Schema::Object(s) = s.as_mut() {
                sv.visit_schema_object(definitions, s);
            }
        }
        Some(SingleOrVec::Vec(vec)) => {
            for s in vec {
                if let Schema::Object(s) = s {
                    sv.visit_schema_object(definitions, s);
                }
            }
        }
    }
}
