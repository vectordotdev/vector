use tracing::debug;

use super::{get_cleaned_schema_reference, Map, RootSchema, Schema, SchemaObject, SingleOrVec};

/// Trait used to recursively modify a constructed schema and its subschemas.
pub trait Visitor: std::fmt::Debug {
    /// Override this method to modify a [`RootSchema`] and (optionally) its subschemas.
    ///
    /// When overriding this method, you will usually want to call the [`visit_root_schema`] function to visit subschemas.
    fn visit_root_schema(&mut self, root: &mut RootSchema) {
        visit_root_schema(self, root);
    }

    /// Override this method to modify a [`Schema`] and (optionally) its subschemas.
    ///
    /// When overriding this method, you will usually want to call the [`visit_schema`] function to visit subschemas.
    fn visit_schema(&mut self, definitions: &mut Map<String, Schema>, schema: &mut Schema) {
        visit_schema(self, definitions, schema);
    }

    /// Override this method to modify a [`SchemaObject`] and (optionally) its subschemas.
    ///
    /// When overriding this method, you will usually want to call the [`visit_schema_object`] function to visit subschemas.
    fn visit_schema_object(
        &mut self,
        definitions: &mut Map<String, Schema>,
        schema: &mut SchemaObject,
    ) {
        visit_schema_object(self, definitions, schema);
    }
}

/// Visits all subschemas of the [`RootSchema`].
pub fn visit_root_schema<V: Visitor + ?Sized>(v: &mut V, root: &mut RootSchema) {
    v.visit_schema_object(&mut root.definitions, &mut root.schema);
}

/// Visits all subschemas of the [`Schema`].
pub fn visit_schema<V: Visitor + ?Sized>(
    v: &mut V,
    definitions: &mut Map<String, Schema>,
    schema: &mut Schema,
) {
    if let Schema::Object(schema) = schema {
        v.visit_schema_object(definitions, schema);
    }
}

/// Visits all subschemas of the [`SchemaObject`].
pub fn visit_schema_object<V: Visitor + ?Sized>(
    v: &mut V,
    definitions: &mut Map<String, Schema>,
    schema: &mut SchemaObject,
) {
    if schema.reference.is_some() {
        with_resolved_schema_reference(
            definitions,
            schema,
            |defs, schema_ref, referenced_schema| {
                debug!(referent = schema_ref, "Visiting schema reference.");

                v.visit_schema(defs, referenced_schema);
            },
        )
    }

    if let Some(sub) = &mut schema.subschemas {
        visit_vec(v, definitions, &mut sub.all_of);
        visit_vec(v, definitions, &mut sub.any_of);
        visit_vec(v, definitions, &mut sub.one_of);
        visit_box(v, definitions, &mut sub.not);
        visit_box(v, definitions, &mut sub.if_schema);
        visit_box(v, definitions, &mut sub.then_schema);
        visit_box(v, definitions, &mut sub.else_schema);
    }

    if let Some(arr) = &mut schema.array {
        visit_single_or_vec(v, definitions, &mut arr.items);
        visit_box(v, definitions, &mut arr.additional_items);
        visit_box(v, definitions, &mut arr.contains);
    }

    if let Some(obj) = &mut schema.object {
        visit_map_values(v, definitions, &mut obj.properties);
        visit_map_values(v, definitions, &mut obj.pattern_properties);
        visit_box(v, definitions, &mut obj.additional_properties);
        visit_box(v, definitions, &mut obj.property_names);
    }
}

fn visit_box<V: Visitor + ?Sized>(
    v: &mut V,
    definitions: &mut Map<String, Schema>,
    target: &mut Option<Box<Schema>>,
) {
    if let Some(s) = target {
        v.visit_schema(definitions, s);
    }
}

fn visit_vec<V: Visitor + ?Sized>(
    v: &mut V,
    definitions: &mut Map<String, Schema>,
    target: &mut Option<Vec<Schema>>,
) {
    if let Some(vec) = target {
        for s in vec {
            v.visit_schema(definitions, s);
        }
    }
}

fn visit_map_values<V: Visitor + ?Sized>(
    v: &mut V,
    definitions: &mut Map<String, Schema>,
    target: &mut Map<String, Schema>,
) {
    for s in target.values_mut() {
        v.visit_schema(definitions, s);
    }
}

fn visit_single_or_vec<V: Visitor + ?Sized>(
    v: &mut V,
    definitions: &mut Map<String, Schema>,
    target: &mut Option<SingleOrVec<Schema>>,
) {
    match target {
        None => {}
        Some(SingleOrVec::Single(s)) => v.visit_schema(definitions, s),
        Some(SingleOrVec::Vec(vec)) => {
            for s in vec {
                v.visit_schema(definitions, s);
            }
        }
    }
}

pub fn with_resolved_schema_reference<F>(
    definitions: &mut Map<String, Schema>,
    schema: &mut SchemaObject,
    f: F,
) where
    F: FnOnce(&mut Map<String, Schema>, &str, &mut Schema),
{
    if let Some(reference) = schema.reference.as_ref() {
        let schema_def_key = get_cleaned_schema_reference(reference);
        let mut referenced_schema = definitions
            .get(schema_def_key)
            .cloned()
            .expect("schema reference should exist");

        f(definitions, schema_def_key, &mut referenced_schema);

        definitions.insert(schema_def_key.to_string(), referenced_schema);
    }
}
