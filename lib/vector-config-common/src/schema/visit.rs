use super::{Map, RootSchema, Schema, SchemaObject, SingleOrVec};

/// Trait used to recursively modify a constructed schema and its subschemas.
pub trait Visitor: std::fmt::Debug {
    /// Override this method to modify a [`RootSchema`] and (optionally) its subschemas.
    ///
    /// When overriding this method, you will usually want to call the [`visit_root_schema`] function to visit subschemas.
    fn visit_root_schema(&mut self, root: &mut RootSchema) {
        visit_root_schema(self, root)
    }

    /// Override this method to modify a [`Schema`] and (optionally) its subschemas.
    ///
    /// When overriding this method, you will usually want to call the [`visit_schema`] function to visit subschemas.
    fn visit_schema(&mut self, schema: &mut Schema) {
        visit_schema(self, schema)
    }

    /// Override this method to modify a [`SchemaObject`] and (optionally) its subschemas.
    ///
    /// When overriding this method, you will usually want to call the [`visit_schema_object`] function to visit subschemas.
    fn visit_schema_object(&mut self, schema: &mut SchemaObject) {
        visit_schema_object(self, schema)
    }
}

/// Visits all subschemas of the [`RootSchema`].
pub fn visit_root_schema<V: Visitor + ?Sized>(v: &mut V, root: &mut RootSchema) {
    v.visit_schema_object(&mut root.schema);
    visit_map_values(v, &mut root.definitions);
}

/// Visits all subschemas of the [`Schema`].
pub fn visit_schema<V: Visitor + ?Sized>(v: &mut V, schema: &mut Schema) {
    if let Schema::Object(schema) = schema {
        v.visit_schema_object(schema)
    }
}

/// Visits all subschemas of the [`SchemaObject`].
pub fn visit_schema_object<V: Visitor + ?Sized>(v: &mut V, schema: &mut SchemaObject) {
    if let Some(sub) = &mut schema.subschemas {
        visit_vec(v, &mut sub.all_of);
        visit_vec(v, &mut sub.any_of);
        visit_vec(v, &mut sub.one_of);
        visit_box(v, &mut sub.not);
        visit_box(v, &mut sub.if_schema);
        visit_box(v, &mut sub.then_schema);
        visit_box(v, &mut sub.else_schema);
    }

    if let Some(arr) = &mut schema.array {
        visit_single_or_vec(v, &mut arr.items);
        visit_box(v, &mut arr.additional_items);
        visit_box(v, &mut arr.contains);
    }

    if let Some(obj) = &mut schema.object {
        visit_map_values(v, &mut obj.properties);
        visit_map_values(v, &mut obj.pattern_properties);
        visit_box(v, &mut obj.additional_properties);
        visit_box(v, &mut obj.property_names);
    }
}

fn visit_box<V: Visitor + ?Sized>(v: &mut V, target: &mut Option<Box<Schema>>) {
    if let Some(s) = target {
        v.visit_schema(s)
    }
}

fn visit_vec<V: Visitor + ?Sized>(v: &mut V, target: &mut Option<Vec<Schema>>) {
    if let Some(vec) = target {
        for s in vec {
            v.visit_schema(s)
        }
    }
}

fn visit_map_values<V: Visitor + ?Sized>(v: &mut V, target: &mut Map<String, Schema>) {
    for s in target.values_mut() {
        v.visit_schema(s)
    }
}

fn visit_single_or_vec<V: Visitor + ?Sized>(v: &mut V, target: &mut Option<SingleOrVec<Schema>>) {
    match target {
        None => {}
        Some(SingleOrVec::Single(s)) => v.visit_schema(s),
        Some(SingleOrVec::Vec(vec)) => {
            for s in vec {
                v.visit_schema(s)
            }
        }
    }
}
