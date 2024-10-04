use std::collections::{HashMap, HashSet};

use serde_json::Value;
use tracing::debug;
use vector_config_common::schema::{visit::Visitor, *};

use crate::schema::visitors::merge::Mergeable;

use super::scoped_visit::{
    visit_schema_object_scoped, SchemaReference, SchemaScopeStack, ScopedVisitor,
};

/// A visitor that inlines schema references where the referenced schema is only referenced once.
///
/// In many cases, the schema generation will produce schema definitions where either generics or
/// flattening are involved, which leads to schema definitions that may only be referenced by one
/// other schema definition, and so on.
///
/// This is suboptimal due to the "pointer chasing" involved to resolve those schema references,
/// when there's no reason to inherently have a schema be defined such that it can be referenced.
///
/// This visitor collects a list of all schema references, and for any schemas which are referenced
/// only once, will replace those references by inlining the referenced schema directly, and
/// deleting the schema definition from the root definitions.
#[derive(Debug, Default)]
pub struct InlineSingleUseReferencesVisitor {
    eligible_to_inline: HashSet<String>,
}

impl InlineSingleUseReferencesVisitor {
    pub fn from_settings(_: &SchemaSettings) -> Self {
        Self {
            eligible_to_inline: HashSet::new(),
        }
    }
}

impl Visitor for InlineSingleUseReferencesVisitor {
    fn visit_root_schema(&mut self, root: &mut RootSchema) {
        // Build a map of schema references and the number of times they're referenced through the
        // entire schema, by visiting the root schema in a recursive fashion, using a helper visitor.
        let mut occurrence_visitor = OccurrenceVisitor::default();
        occurrence_visitor.visit_root_schema(root);
        let occurrence_map = occurrence_visitor.into_occurrences();

        self.eligible_to_inline = occurrence_map
            .into_iter()
            // Filter out any schemas which have more than one occurrence, as naturally, we're
            // trying to inline single-use schema references. :)
            .filter_map(|(def_name, occurrences)| (occurrences == 1).then_some(def_name))
            // However, we'll also filter out some specific schema definitions which are only
            // referenced once, specifically: component base types and component types themselves.
            //
            // We do this as a lot of the tooling that parses the schema to generate documentation,
            // and the like, depends on these schemas existing in the top-level definitions for easy
            // lookup.
            .filter(|def_name| {
                let schema = root
                    .definitions
                    .get(def_name.as_ref())
                    .and_then(Schema::as_object)
                    .expect("schema definition must exist");

                is_inlineable_schema(def_name.as_ref(), schema)
            })
            .map(|s| s.as_ref().to_string())
            .collect::<HashSet<_>>();

        // Now run our own visitor logic, which will use the inline eligibility to determine if a
        // schema reference in a being-visited schema should be replaced inline with the original
        // referenced schema, in turn removing the schema definition.
        visit::visit_root_schema(self, root);

        // Now remove all of the definitions for schemas that were eligible for inlining.
        for schema_def_name in self.eligible_to_inline.drain() {
            debug!(
                referent = schema_def_name,
                "Removing schema definition from root schema."
            );

            root.definitions
                .remove(&schema_def_name)
                .expect("referenced schema must exist in definitions");
        }
    }

    fn visit_schema_object(
        &mut self,
        definitions: &mut Map<String, Schema>,
        schema: &mut SchemaObject,
    ) {
        // Recursively visit this schema first.
        visit::visit_schema_object(self, definitions, schema);

        // If this schema has a schema reference, see if it's in our inline eligibility map. If so,
        // we remove the referenced schema from the definitions, and then merge it into the current
        // schema, after removing the `$ref` field.
        if let Some(schema_ref) = schema.reference.as_ref().cloned() {
            let schema_ref = get_cleaned_schema_reference(&schema_ref);
            if self.eligible_to_inline.contains(schema_ref) {
                let referenced_schema = definitions
                    .get(schema_ref)
                    .expect("referenced schema must exist in definitions");

                if let Schema::Object(referenced_schema) = referenced_schema {
                    debug!(
                        referent = schema_ref,
                        "Inlining eligible schema reference into current schema."
                    );

                    schema.reference = None;
                    schema.merge(referenced_schema);
                }
            }
        }
    }
}

fn is_inlineable_schema(definition_name: &str, schema: &SchemaObject) -> bool {
    static DISALLOWED_SCHEMAS: &[&str] = &[
        "vector::sources::Sources",
        "vector::transforms::Transforms",
        "vector::sinks::Sinks",
    ];

    // We want to avoid inlining all of the relevant top-level types used for defining components:
    // the "outer" types (i.e. `SinkOuter<T>`), the enum/collection types (i.e. the big `Sources`
    // enum), and the component configuration types themselves (i.e. `AmqpSinkConfig`).
    //
    // There's nothing _technically_ wrong with doing so, but it would break downstream consumers of
    // the schema that parse it in order to extract the individual components and other
    // component-specific metadata.
    let is_component_base = get_schema_metadata_attr(schema, "docs::component_base_type").is_some();
    let is_component = get_schema_metadata_attr(schema, "docs::component_type").is_some();

    let is_allowed_schema = !DISALLOWED_SCHEMAS.contains(&definition_name);

    !is_component_base && !is_component && is_allowed_schema
}

#[derive(Debug, Default)]
struct OccurrenceVisitor {
    scope_stack: SchemaScopeStack,
    occurrence_map: HashMap<SchemaReference, HashSet<SchemaReference>>,
}

impl OccurrenceVisitor {
    fn into_occurrences(self) -> HashMap<SchemaReference, usize> {
        self.occurrence_map
            .into_iter()
            .map(|(k, v)| (k, v.len()))
            .collect()
    }
}

impl Visitor for OccurrenceVisitor {
    fn visit_schema_object(
        &mut self,
        definitions: &mut Map<String, Schema>,
        schema: &mut SchemaObject,
    ) {
        visit_schema_object_scoped(self, definitions, schema);

        if let Some(current_schema_ref) = schema.reference.as_ref() {
            // Track the named "parent" schema for the schema we're currently visiting so that if we
            // visit this schema again, we don't double count any schema references that it has. The
            // "parent" schema is simply the closest ancestor schema that was itself a schema
            // reference, or "Root", which represents the oldest schema ancestor in the document.
            //
            // This lets us couple with scenarios where schema A references schema B, and is the
            // only actual direct schema reference to schema B, but due to multiple schemas
            // referencing schema A, would otherwise lead to both A and B being visited multiple
            // times.
            let current_parent_schema_ref = self.get_current_schema_scope().clone();
            let current_schema_ref = get_cleaned_schema_reference(current_schema_ref);

            let occurrences = self
                .occurrence_map
                .entry(current_schema_ref.into())
                .or_default();
            occurrences.insert(current_parent_schema_ref);
        }
    }
}

impl ScopedVisitor for OccurrenceVisitor {
    fn push_schema_scope<S: Into<SchemaReference>>(&mut self, scope: S) {
        self.scope_stack.push(scope.into());
    }

    fn pop_schema_scope(&mut self) {
        self.scope_stack.pop().expect("stack was empty during pop");
    }

    fn get_current_schema_scope(&self) -> &SchemaReference {
        self.scope_stack.current().unwrap_or(&SchemaReference::Root)
    }
}

fn get_schema_metadata_attr<'a>(schema: &'a SchemaObject, key: &str) -> Option<&'a Value> {
    schema
        .extensions
        .get("_metadata")
        .and_then(|metadata| metadata.get(key))
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use vector_config_common::schema::visit::Visitor;

    use crate::schema::visitors::test::{as_schema, assert_schemas_eq};

    use super::InlineSingleUseReferencesVisitor;

    #[test]
    fn no_refs() {
        let mut actual_schema = as_schema(json!({
            "type": "object",
            "properties": {
                "a": { "type": "string" }
            }
        }));

        let expected_schema = actual_schema.clone();

        let mut visitor = InlineSingleUseReferencesVisitor::default();
        visitor.visit_root_schema(&mut actual_schema);

        assert_schemas_eq(expected_schema, actual_schema);
    }

    #[test]
    fn single_ref_single_usage() {
        let mut actual_schema = as_schema(json!({
            "$ref": "#/definitions/simple",
            "definitions": {
                "simple": {
                    "type": "object",
                    "properties": {
                        "a": { "type": "string" }
                    }
                }
            }
        }));

        let mut visitor = InlineSingleUseReferencesVisitor::default();
        visitor.visit_root_schema(&mut actual_schema);

        let expected_schema = as_schema(json!({
            "type": "object",
            "properties": {
                "a": { "type": "string" }
            },
        }));

        assert_schemas_eq(expected_schema, actual_schema);
    }

    // TODO(tobz): These two tests are ignored because the inliner currently works off of schema
    // reference scopes, so two object properties within the same schema don't count as two
    // instances of a schema being referenced.
    //
    // We need to refactor schema scopes to be piecemeal extensible more in the way of how
    // `jsonschema` does it rather than an actual stack.... the current approach is good enough for
    // now, but not optimal in the way that it could be.
    //
    // These are here for when I improve the situation after getting this merged.
    #[test]
    #[ignore]
    fn single_ref_multiple_usages() {
        let mut actual_schema = as_schema(json!({
            "definitions": {
                "simple": {
                    "type": "object",
                    "properties": {
                        "a": { "type": "string" }
                    }
                }
            },
            "type": "object",
            "properties": {
                "a": { "$ref": "#/definitions/simple" },
                "b": { "$ref": "#/definitions/simple" }
            }
        }));

        let expected_schema = actual_schema.clone();

        let mut visitor = InlineSingleUseReferencesVisitor::default();
        visitor.visit_root_schema(&mut actual_schema);

        assert_schemas_eq(expected_schema, actual_schema);
    }

    #[test]
    #[ignore]
    fn multiple_refs_mixed_usages() {
        let mut actual_schema = as_schema(json!({
            "definitions": {
                "simple": {
                    "type": "object",
                    "properties": {
                        "a": { "type": "string" }
                    }
                },
                "advanced": {
                    "type": "object",
                    "properties": {
                        "b": { "type": "string" }
                    }
                }
            },
            "type": "object",
            "properties": {
                "a": { "$ref": "#/definitions/simple" },
                "b": { "$ref": "#/definitions/simple" },
                "c": { "$ref": "#/definitions/advanced" },
            }
        }));

        let mut visitor = InlineSingleUseReferencesVisitor::default();
        visitor.visit_root_schema(&mut actual_schema);

        let expected_schema = as_schema(json!({
            "definitions": {
                "simple": {
                    "type": "object",
                    "properties": {
                        "a": { "type": "string" }
                    }
                }
            },
            "type": "object",
            "properties": {
                "a": { "$ref": "#/definitions/simple" },
                "b": { "$ref": "#/definitions/simple" },
                "c": {
                    "type": "object",
                    "properties": {
                        "b": { "type": "string" }
                    }
                }
            }
        }));

        assert_schemas_eq(expected_schema, actual_schema);
    }
}
