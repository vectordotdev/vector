use std::{
    collections::{HashMap, HashSet},
    convert::identity,
};

use tracing::debug;
use vector_config_common::schema::{
    visit::{with_resolved_schema_reference, Visitor},
    *,
};

use crate::schema::visitors::merge::Mergeable;

use super::scoped_visit::{
    visit_schema_object_scoped, SchemaReference, SchemaScopeStack, ScopedVisitor,
};

/// A visitor that marks schemas as closed by disallowing unknown properties via
/// `unevaluatedProperties`.
///
/// This is the equivalent of `serde`'s `deny_unknown_fields` attribute: instead of only validating
/// the properties specified in the schema, and ignoring any properties present in the JSON
/// instance, any unevaluated properties are considered an error.
///
/// This visitor selectively marks schemas with `unevaluatedProperties: false` in order to ensure
/// that unknown properties are not allowed, but also in a way that doesn't interact incorrectly
/// with advanced subschema validation, such as `oneOf` or `allOf`, as `unevaluatedProperties`
/// cannot simply be applied to any and all schemas indiscriminately.
#[derive(Debug, Default)]
pub struct DisallowUnevaluatedPropertiesVisitor {
    scope_stack: SchemaScopeStack,
    eligible_to_flatten: HashMap<String, HashSet<SchemaReference>>,
}

impl DisallowUnevaluatedPropertiesVisitor {
    pub fn from_settings(_: &SchemaSettings) -> Self {
        Self {
            scope_stack: SchemaScopeStack::default(),
            eligible_to_flatten: HashMap::new(),
        }
    }
}

impl Visitor for DisallowUnevaluatedPropertiesVisitor {
    fn visit_root_schema(&mut self, root: &mut RootSchema) {
        let eligible_to_flatten = build_closed_schema_flatten_eligibility_mappings(root);

        debug!(
            "Found {} referents eligible for flattening: {:?}",
            eligible_to_flatten.len(),
            eligible_to_flatten,
        );

        self.eligible_to_flatten = eligible_to_flatten;

        visit::visit_root_schema(self, root);
    }

    fn visit_schema_object(
        &mut self,
        definitions: &mut Map<String, Schema>,
        schema: &mut SchemaObject,
    ) {
        // If this schema has a schema reference, check our flattening eligibility map to figure out
        // if we need to merge it in.
        //
        // When a given schema reference (the actual target of `$ref`) is eligible for flattening in
        // a given schema (what we're currently visiting) then it means that this schema would,
        // based on its composition, lead to the schema reference either being marked or unmarked.
        //
        // We flatten the schema reference into this schema to avoid that from occurring, and we do
        // so based on whichever group of referrers -- the schemas which reference the particular
        // target schema -- is smaller, such that we do the minimum number of flattenings per target
        // schema, to keep the schema as small as we reasonably can.
        if let Some(reference) = schema.reference.as_ref() {
            let current_parent_schema_ref = self.get_current_schema_scope();

            if let Some(referrers) = self.eligible_to_flatten.get(reference) {
                if referrers.contains(current_parent_schema_ref) {
                    let current_schema_ref = get_cleaned_schema_reference(reference);
                    let referenced_schema = definitions
                        .get(current_schema_ref)
                        .expect("schema definition must exist");

                    debug!(
                        referent = current_schema_ref,
                        referrer = current_parent_schema_ref.as_ref(),
                        "Found eligible referent/referrer mapping."
                    );

                    if let Schema::Object(referenced_schema) = referenced_schema {
                        debug!(
                            referent = current_schema_ref,
                            referrer = current_parent_schema_ref.as_ref(),
                            "Flattening referent into referrer."
                        );

                        schema.reference = None;
                        schema.merge(referenced_schema);
                    }
                }
            }
        }

        // Visit the schema object first so that we recurse the overall schema in a depth-first
        // fashion, marking eligible object schemas as closed.
        visit_schema_object_scoped(self, definitions, schema);

        // Next, see if this schema has any subschema validation: `allOf`, `oneOf`, or `anyOf`.
        //
        // If so, we ensure that none of them have `unevaluatedProperties` set at all. We do this
        // because subschema validation involves each subschema seeing the entire JSON instance, or
        // seeing a value that's unrelated: we know that some schemas in a `oneOf` won't match, and
        // that's fine, but if they're marked with `unevaluatedProperties: false`, they'll fail...
        // which is why we remove that from the subschemas themselves but essentially hoist it up
        // to the level of the `allOf`/`oneOf`/`anyOf`, where it can apply the correct behavior.
        let mut had_relevant_subschemas = false;
        if let Some(subschema) = schema.subschemas.as_mut() {
            let subschemas = get_object_subschemas_from_parent_mut(subschema.as_mut());
            for subschema in subschemas {
                had_relevant_subschemas = true;

                unmark_or_flatten_schema(definitions, subschema);
            }
        }

        // If we encountered any subschema validation, or if this schema itself is an object schema,
        // mark the schema as closed by setting `unevaluatedProperties` to `false`.
        if had_relevant_subschemas || is_object_schema(schema) {
            mark_schema_closed(schema);
        }
    }
}

impl ScopedVisitor for DisallowUnevaluatedPropertiesVisitor {
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

fn unmark_or_flatten_schema(definitions: &mut Map<String, Schema>, schema: &mut SchemaObject) {
    // If the schema is an object schema, we'll unset `unevaluatedProperties` directly.
    // If it isn't an object schema, we'll see if the subschema is actually a schema
    // reference, and if so, we'll make sure to unset `unevaluatedProperties` on the
    // resolved schema reference itself.
    //
    // Like the top-level schema reference logic, this ensures the schema definition is
    // updated for subsequent resolution.
    if let Some(object) = schema.object.as_mut() {
        debug!("Unmarked object subschema directly.");

        object.unevaluated_properties = Some(Box::new(Schema::Bool(true)));
    } else {
        with_resolved_schema_reference(definitions, schema, |_, schema_ref, resolved| {
            if let Schema::Object(resolved) = resolved {
                if let Some(object) = resolved.object.as_mut() {
                    debug!(
                        referent = schema_ref,
                        "Unmarked subschema by traversing schema reference."
                    );

                    object.unevaluated_properties = Some(Box::new(Schema::Bool(true)));
                }
            }
        });
    }
}

/// A referent schema that carries the chance of being unmarking by its referrer.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct MarkableReferent {
    // Whether or not the referent would be unmarked by the referrer.
    would_unmark: bool,

    /// The referent schema.
    referent: SchemaReference,
}

impl MarkableReferent {
    fn would_unmark<R: Into<SchemaReference>>(referent: R) -> Self {
        Self {
            would_unmark: true,
            referent: referent.into(),
        }
    }

    fn would_not_unmark<R: Into<SchemaReference>>(referent: R) -> Self {
        Self {
            would_unmark: false,
            referent: referent.into(),
        }
    }

    fn with_new_referent<R: Into<SchemaReference>>(&self, new_referent: R) -> Self {
        Self {
            would_unmark: self.would_unmark,
            referent: new_referent.into(),
        }
    }
}

fn build_closed_schema_flatten_eligibility_mappings(
    root_schema: &RootSchema,
) -> HashMap<String, HashSet<SchemaReference>> {
    // For all definitions, visit _just_ the defined schema (no recursing) and build a map of child
    // definitions -> (mark eligibility, [(parent definition, would_unmark)]), such that we know
    // exactly which schemas refer to any given schema definition and if they would lead to the
    // child schema being marked as `unevaluatedProperties: false`.
    //
    // We would filter out any child definitions that aren't eligible to be marked. For the
    // remaining child schemas, we group the parent definitions by `would_unmark`, which indicates
    // whether or not the given parent definition would cause the child definition to be unmarked.
    //
    // As an example, we would expect a parent schema referring to a child schema via `allOf` to
    // unmark the child schema, while a parent schema referring to a child schema within a specific
    // property to not unmark the child schema.
    //
    // With the grouped parent definitions, take the smaller of the two groups. This represents the
    // set of parent schemas that we will indicate as needing to use a flattened version of the
    // child schema when we execute our primary visit logic.

    // Iterate over all definitions, and once more for the root schema, and generate a map of parent
    // schema -> (would_unmark, child schema).
    let mut parent_to_child = HashMap::new();
    for (definition_name, definition) in &root_schema.definitions {
        // We only care about full-fledged schemas, not boolean schemas.
        let parent_schema = match definition {
            Schema::Bool(_) => continue,
            Schema::Object(schema) => schema,
        };

        debug!(
            "Evaluating schema definition '{}' for markability.",
            definition_name
        );

        // If a schema itself would not be considered markable, then we don't need to consider the
        // eligibility between parent/child since there's nothing to drive the "now unmark the child
        // schemas" logic.
        if !is_markable_schema(&root_schema.definitions, parent_schema) {
            debug!("Schema definition '{}' not markable.", definition_name);
            continue;
        } else {
            debug!(
                "Schema definition '{}' markable. Collecting referents.",
                definition_name
            );
        }

        // Collect all referents for this definition, which includes both property-based referents
        // and subschema-based referents. Property-based referents are not required to be unmarked,
        // while subschema-based referents must be unmarked.
        let mut referents = HashSet::new();
        get_referents(parent_schema, &mut referents);

        debug!(
            "Collected {} referents for '{}': {:?}",
            referents.len(),
            definition_name,
            referents
        );

        // Store the parent/child mapping.
        parent_to_child.insert(SchemaReference::from(definition_name), referents);
    }

    // Collect the referents from the root schema.
    let mut root_referents = HashSet::new();
    get_referents(&root_schema.schema, &mut root_referents);
    parent_to_child.insert(SchemaReference::Root, root_referents);

    // Now we build a reverse map, going from child -> parent. We'll iterate over every child
    // referent, for every parent/child entry, calculating the set of referrers, and if they would
    // require unmarking the child.
    let mut child_to_parent = HashMap::new();
    for (parent_schema_ref, child_referents) in parent_to_child {
        for child_referent in child_referents {
            let entry = child_to_parent
                .entry(child_referent.referent.as_ref().to_string())
                .or_insert_with(HashSet::new);

            // Transform the child referent into a parent referent, which preserves the "would
            // unmark" value but now points to the parent instead, and add it to the list of
            // _referrers_ for the child.
            entry.insert(child_referent.with_new_referent(parent_schema_ref.clone()));
        }
    }

    let mut eligible_to_flatten = HashMap::new();
    for (child_schema_ref, referrers) in child_to_parent {
        // Don't flatten schemas which have less than two referrers.
        if referrers.len() < 2 {
            continue;
        }

        let would_unmark = referrers
            .iter()
            .filter(|r| r.would_unmark)
            .map(|r| r.referent.clone())
            .collect::<HashSet<_>>();
        let would_not_unmark = referrers
            .iter()
            .filter(|r| !r.would_unmark)
            .map(|r| r.referent.clone())
            .collect::<HashSet<_>>();

        if would_not_unmark.len() >= would_unmark.len() {
            eligible_to_flatten.insert(child_schema_ref.to_string(), would_unmark);
        } else {
            eligible_to_flatten.insert(child_schema_ref.to_string(), would_not_unmark);
        }
    }

    eligible_to_flatten
}

/// Determines whether a schema is eligible to be marked.
fn is_markable_schema(definitions: &Map<String, Schema>, schema: &SchemaObject) -> bool {
    // If the schema is an object schema, and does not have`additionalProperties` set, it can be
    // marked, as marking a schema with both `unevaluatedProperties`/`additionalProperties` would
    // otherwise be a logical inconsistency.
    let has_additional_properties = schema
        .object
        .as_ref()
        .and_then(|object| object.additional_properties.as_ref())
        .map(|schema| matches!(schema.as_ref(), Schema::Object(_)))
        .unwrap_or(false);

    if is_object_schema(schema) && !has_additional_properties {
        return true;
    }

    // If the schema uses subschema validation -- specifically: `allOf`, `oneOf`, or `anyOf` -- then
    // it should be marked, so long as one of the subschemas is actually an object schema.
    //
    // If we're dealing with something like a `oneOf` for `Option<T>`, we'll have two
    // subschemas: { "type": "null" } and { "$ref": "#/definitions/T" }. If the schema for `T` is,
    // say, just a scalar schema, instead of an object schema... then it wouldn't be marked, and in
    // turn, we wouldn't need to mark the schema for `Option<T>`: there's no properties at all.
    if let Some(subschema) = schema.subschemas.as_ref() {
        let subschemas = get_object_subschemas_from_parent(subschema).collect::<Vec<_>>();

        debug!("{} subschemas detected.", subschemas.len());

        let has_object_subschema = subschemas
            .iter()
            .any(|schema| is_markable_schema(definitions, schema));
        let has_referenced_object_subschema = subschemas
            .iter()
            .map(|subschema| {
                subschema
                    .reference
                    .as_ref()
                    .and_then(|reference| {
                        let reference = get_cleaned_schema_reference(reference);
                        definitions.get_key_value(reference)
                    })
                    .and_then(|(name, schema)| schema.as_object().map(|schema| (name, schema)))
                    .map_or(false, |(name, schema)| {
                        debug!(
                            "Following schema reference '{}' for subschema markability.",
                            name
                        );
                        is_markable_schema(definitions, schema)
                    })
            })
            .any(identity);

        debug!(
            "Schema {} object subschema(s) and {} referenced subschemas.",
            if has_object_subschema {
                "has"
            } else {
                "does not have"
            },
            if has_referenced_object_subschema {
                "has"
            } else {
                "does not have"
            },
        );

        if has_object_subschema || has_referenced_object_subschema {
            return true;
        }
    }

    false
}

/// Collects all referents from the given parent schema, and inserts them to `referents`.
///
/// Property schemas from `properties`, `patternProperties`, and `additionalProperties` are checked.
/// Any such referents in a property schema are do not need to be unmarked as the "chain" between
/// parent/child is broken implicitly by the property-level scoping of the value they would be given
/// to validate.
///
/// Subschemas from `allOf`, `oneOf`, and `anyOf` are also checked. As subschema validation implies
/// that each subschema will be given the same value to validate, even if the subschema only
/// represents a slice of the parent schema, there is a link between parent/child that requires the
/// child to be unmarked so that the parent can be marked to enforce `unevaluatedProperties` at the
/// correct scope.
///
/// This function will recurse a schema object entirely, in terms of property schemas and
/// subschemas, but will not recurse through schema references.
fn get_referents(parent_schema: &SchemaObject, referents: &mut HashSet<MarkableReferent>) {
    if let Some(parent_object) = parent_schema.object.as_ref() {
        // For both `properties` and `patternProperties`, collect the schema reference, if any, from
        // all property schemas.
        for (_, property_schema) in parent_object
            .properties
            .iter()
            .chain(parent_object.pattern_properties.iter())
        {
            if let Some(child_schema) = property_schema.as_object() {
                if let Some(child_schema_ref) = child_schema.reference.as_ref() {
                    referents.insert(MarkableReferent::would_not_unmark(child_schema_ref));
                } else {
                    get_referents(child_schema, referents);
                }
            }
        }

        // For `additionalProperties`, if present and defined as a schema object, collect the schema
        // reference if one is set.
        if let Some(additional_properties) = parent_object.additional_properties.as_ref() {
            if let Some(child_schema) = additional_properties.as_ref().as_object() {
                if let Some(child_schema_ref) = child_schema.reference.as_ref() {
                    referents.insert(MarkableReferent::would_not_unmark(child_schema_ref));
                } else {
                    get_referents(child_schema, referents);
                }
            }
        }
    }

    if let Some(subschema) = parent_schema.subschemas.as_ref() {
        // For `allOf`, `oneOf`, and `anyOf`, collect the schema reference, if any, from their
        // respective subschemas.
        for subschema in get_object_subschemas_from_parent(subschema) {
            if let Some(child_schema_ref) = subschema.reference.as_ref() {
                referents.insert(MarkableReferent::would_unmark(child_schema_ref));
            } else {
                get_referents(subschema, referents);
            }
        }
    }
}

fn get_object_subschemas_from_parent(
    subschema: &SubschemaValidation,
) -> impl Iterator<Item = &SchemaObject> {
    [
        subschema.all_of.as_ref(),
        subschema.one_of.as_ref(),
        subschema.any_of.as_ref(),
    ]
    .into_iter()
    .flatten()
    .flatten()
    .filter_map(Schema::as_object)
}

fn get_object_subschemas_from_parent_mut(
    subschema: &mut SubschemaValidation,
) -> impl Iterator<Item = &mut SchemaObject> {
    [
        subschema.all_of.as_mut(),
        subschema.one_of.as_mut(),
        subschema.any_of.as_mut(),
    ]
    .into_iter()
    .flatten()
    .flatten()
    .filter_map(Schema::as_object_mut)
}

fn mark_schema_closed(schema: &mut SchemaObject) {
    // Make sure this schema doesn't also have `additionalProperties` set to a non-boolean schema,
    // as it would be a logical inconsistency to then also set `unevaluatedProperties` to `false`.
    //
    // TODO: We may want to consider dropping `additionalProperties` entirely here if it's a boolean
    // schema, as `unevaluatedProperties` would provide the equivalent behavior, and it avoids us
    // running into weird validation issues where `additionalProperties` gets used in situations it
    // can't handle, the same ones we switched to using `unevaluatedProperties` for in the first
    // place... but realistically, we don't ourselves generated boolean schemas for
    // `additionalProperties` through normal means, so it's not currently an issue that should even
    // occur.
    if let Some(Schema::Object(_)) = schema
        .object()
        .additional_properties
        .as_ref()
        .map(|v| v.as_ref())
    {
        return;
    }

    // As well, if `unevaluatedProperties` is already set, then we don't do anything. By default,
    // the field on the Rust side will be unset, so if it's been set explicitly, that means another
    // usage of this schema requires that it not be set to `false`.
    if schema
        .object
        .as_ref()
        .and_then(|object| object.unevaluated_properties.as_ref())
        .is_some()
    {
        return;
    }

    schema.object().unevaluated_properties = Some(Box::new(Schema::Bool(false)));
}

fn schema_type_matches(
    schema: &SchemaObject,
    instance_type: InstanceType,
    allow_multiple: bool,
) -> bool {
    match schema.instance_type.as_ref() {
        Some(sov) => match sov {
            SingleOrVec::Single(inner) => inner.as_ref() == &instance_type,
            SingleOrVec::Vec(inner) => inner.contains(&instance_type) && allow_multiple,
        },
        None => false,
    }
}

fn is_object_schema(schema: &SchemaObject) -> bool {
    schema_type_matches(schema, InstanceType::Object, true)
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use vector_config_common::schema::visit::Visitor;

    use crate::schema::visitors::test::{as_schema, assert_schemas_eq};

    use super::DisallowUnevaluatedPropertiesVisitor;

    #[test]
    fn basic_object_schema() {
        let mut actual_schema = as_schema(json!({
            "type": "object",
            "properties": {
                "a": { "type": "string" }
            }
        }));

        let mut visitor = DisallowUnevaluatedPropertiesVisitor::default();
        visitor.visit_root_schema(&mut actual_schema);

        let expected_schema = as_schema(json!({
            "type": "object",
            "properties": {
                "a": { "type": "string" }
            },
            "unevaluatedProperties": false
        }));

        assert_schemas_eq(expected_schema, actual_schema);
    }

    #[test]
    fn basic_object_schema_through_ref() {
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

        let mut visitor = DisallowUnevaluatedPropertiesVisitor::default();
        visitor.visit_root_schema(&mut actual_schema);

        let expected_schema = as_schema(json!({
            "$ref": "#/definitions/simple",
            "definitions": {
                "simple": {
                    "type": "object",
                    "properties": {
                        "a": { "type": "string" }
                    },
                    "unevaluatedProperties": false
                }
            }
        }));

        assert_schemas_eq(expected_schema, actual_schema);
    }

    #[test]
    fn all_of_with_basic_object_schemas() {
        let mut actual_schema = as_schema(json!({
            "type": "object",
            "allOf": [{
                "type": "object",
                "properties": {
                    "a": { "type": "string" }
                }
            },
            {
                "type": "object",
                "properties": {
                    "b": { "type": "string" }
                }
            }]
        }));

        let mut visitor = DisallowUnevaluatedPropertiesVisitor::default();
        visitor.visit_root_schema(&mut actual_schema);

        let expected_schema = as_schema(json!({
            "type": "object",
            "allOf": [{
                "type": "object",
                "properties": {
                    "a": { "type": "string" }
                }
            },
            {
                "type": "object",
                "properties": {
                    "b": { "type": "string" }
                }
            }],
            "unevaluatedProperties": false
        }));

        assert_schemas_eq(expected_schema, actual_schema);
    }

    #[test]
    fn one_of_with_basic_object_schemas() {
        let mut actual_schema = as_schema(json!({
            "type": "object",
            "oneOf": [{
                "type": "object",
                "properties": {
                    "a": { "type": "string" }
                }
            },
            {
                "type": "object",
                "properties": {
                    "b": { "type": "string" }
                }
            }]
        }));

        let mut visitor = DisallowUnevaluatedPropertiesVisitor::default();
        visitor.visit_root_schema(&mut actual_schema);

        let expected_schema = as_schema(json!({
            "type": "object",
            "oneOf": [{
                "type": "object",
                "properties": {
                    "a": { "type": "string" }
                }
            },
            {
                "type": "object",
                "properties": {
                    "b": { "type": "string" }
                }
            }],
            "unevaluatedProperties": false
        }));

        assert_schemas_eq(expected_schema, actual_schema);
    }

    #[test]
    fn any_of_with_basic_object_schemas() {
        let mut actual_schema = as_schema(json!({
            "type": "object",
            "anyOf": [{
                "type": "object",
                "properties": {
                    "a": { "type": "string" }
                }
            },
            {
                "type": "object",
                "properties": {
                    "b": { "type": "string" }
                }
            }]
        }));

        let mut visitor = DisallowUnevaluatedPropertiesVisitor::default();
        visitor.visit_root_schema(&mut actual_schema);

        let expected_schema = as_schema(json!({
            "type": "object",
            "anyOf": [{
                "type": "object",
                "properties": {
                    "a": { "type": "string" }
                }
            },
            {
                "type": "object",
                "properties": {
                    "b": { "type": "string" }
                }
            }],
            "unevaluatedProperties": false
        }));

        assert_schemas_eq(expected_schema, actual_schema);
    }

    #[test]
    fn ignores_object_schema_with_non_boolean_additional_properties() {
        let mut actual_schema = as_schema(json!({
            "type": "object",
            "properties": {
                "a": { "type": "string" }
            },
            "additionalProperties": { "type": "number" }
        }));
        let expected_schema = actual_schema.clone();

        let mut visitor = DisallowUnevaluatedPropertiesVisitor::default();
        visitor.visit_root_schema(&mut actual_schema);

        assert_schemas_eq(expected_schema, actual_schema);
    }

    #[test]
    fn object_schema_with_boolean_additional_properties() {
        let mut actual_schema = as_schema(json!({
            "type": "object",
            "properties": {
                "a": { "type": "string" }
            },
            "additionalProperties": false
        }));

        let mut visitor = DisallowUnevaluatedPropertiesVisitor::default();
        visitor.visit_root_schema(&mut actual_schema);

        let expected_schema = as_schema(json!({
            "type": "object",
            "properties": {
                "a": { "type": "string" }
            },
            "additionalProperties": false,
            "unevaluatedProperties": false
        }));

        assert_schemas_eq(expected_schema, actual_schema);
    }

    #[test]
    fn all_of_with_object_props_using_schema_refs() {
        let mut actual_schema = as_schema(json!({
            "type": "object",
            "allOf": [{
                "type": "object",
                "properties": {
                    "a": { "$ref": "#/definitions/subschema" }
                }
            },
            {
                "type": "object",
                "properties": {
                    "aa": {
                        "type": "object",
                        "properties": {
                            "a": { "$ref": "#/definitions/subschema" }
                        }
                    }
                }
            }],
            "definitions": {
                "subschema": {
                    "type": "object",
                    "properties": {
                        "f": { "type": "string" }
                    }
                }
            }
        }));

        let mut visitor = DisallowUnevaluatedPropertiesVisitor::default();
        visitor.visit_root_schema(&mut actual_schema);

        let expected_schema = as_schema(json!({
            "type": "object",
            "allOf": [{
                "type": "object",
                "properties": {
                    "a": { "$ref": "#/definitions/subschema" }
                }
            },
            {
                "type": "object",
                "properties": {
                    "aa": {
                        "type": "object",
                        "properties": {
                            "a": { "$ref": "#/definitions/subschema" }
                        },
                        "unevaluatedProperties": false
                    }
                }
            }],
            "definitions": {
                "subschema": {
                    "type": "object",
                    "properties": {
                        "f": { "type": "string" }
                    },
                    "unevaluatedProperties": false
                }
            },
            "unevaluatedProperties": false
        }));

        assert_schemas_eq(expected_schema, actual_schema);
    }

    #[test]
    fn conflicting_schema_usages_get_duplicated_and_flattened() {
        let mut actual_schema = as_schema(json!({
            "type": "object",
            "properties": {
                "acks": { "$ref": "#/definitions/acks" },
                "custom_acks": { "$ref": "#/definitions/custom_acks" }
            },
            "definitions": {
                "custom_acks": {
                    "allOf": [{ "type": "object", "properties": { "ack_count": { "type": "number" } } },
                              { "$ref": "#/definitions/acks" }]
                },
                "acks": { "type": "object", "properties": { "enabled": { "type": "boolean" } } }
            }
        }));

        let mut visitor = DisallowUnevaluatedPropertiesVisitor::default();
        visitor.visit_root_schema(&mut actual_schema);

        let expected_schema = as_schema(json!({
            "type": "object",
            "properties": {
                "acks": { "$ref": "#/definitions/acks" },
                "custom_acks": { "$ref": "#/definitions/custom_acks" }
            },
            "definitions": {
                "custom_acks": {
                    "allOf": [
                        { "type": "object", "properties": { "ack_count": { "type": "number" } } },
                        { "type": "object", "properties": { "enabled": { "type": "boolean" } } }
                    ],
                    "unevaluatedProperties": false
                },
                "acks": {
                    "type": "object",
                    "properties": { "enabled": { "type": "boolean" } },
                    "unevaluatedProperties": false
                }
            },
            "unevaluatedProperties": false
        }));

        assert_schemas_eq(expected_schema, actual_schema);
    }

    #[test]
    fn multiple_mark_unmark_references_flattened_efficiently() {
        // This tests that if, for example, one schema reference would be marked and unmarked by
        // multiple referrers, the referrers we choose to flatten the reference on are in the
        // smaller group (i.e. we do as few flattenings as possible).

        let mut actual_schema = as_schema(json!({
            "type": "object",
            "properties": {
                "a": { "$ref": "#/definitions/a" },
                "b": { "$ref": "#/definitions/b" },
                "c": { "$ref": "#/definitions/c" },
                "one": { "$ref": "#/definitions/one" },
                "two": { "$ref": "#/definitions/two" }
            },
            "definitions": {
                "one": {
                    "allOf": [{ "$ref": "#/definitions/c" }]
                },
                "two": {
                    "allOf": [{ "$ref": "#/definitions/b" }, { "$ref": "#/definitions/c" }]
                },
                "a": {
                    "type": "object",
                    "properties": { "a": { "type": "boolean" } }
                },
                "b": {
                    "type": "object",
                    "properties": { "b": { "type": "boolean" } }
                },
                "c": {
                    "type": "object",
                    "properties": { "c": { "type": "boolean" } }
                }
            }
        }));

        let mut visitor = DisallowUnevaluatedPropertiesVisitor::default();
        visitor.visit_root_schema(&mut actual_schema);

        // Expectations:
        // - Schema A is only referenced in an object property, so it's marked normally.
        // - Schema B is referenced twice -- once as an object property and once in a subschema --
        //   so since we prioritize flattening usages that would unmark a schema when the
        //   would-unmark/would-not-unmark counts are equal, schema B is only flattened for the
        //   subschema usage.
        // - Schema C is referenced three times -- once as an object property and twice in a
        //   subschema -- so since there's more would-unmark usages than would-not-unmark usages, we
        //   flatten the smallest group of usages, which is the would-not-unmark group aka object
        //   properties.
        let expected_schema = as_schema(json!({
            "type": "object",
            "properties": {
                "a": { "$ref": "#/definitions/a" },
                "b": { "$ref": "#/definitions/b" },
                "c": {
                    "type": "object",
                    "properties": { "c": { "type": "boolean" } },
                    "unevaluatedProperties": false
                },
                "one": { "$ref": "#/definitions/one" },
                "two": { "$ref": "#/definitions/two" }
            },
            "definitions": {
                "one": {
                    "allOf": [{ "$ref": "#/definitions/c" }],
                    "unevaluatedProperties": false
                },
                "two": {
                    "allOf": [
                        {
                            "type": "object",
                            "properties": { "b": { "type": "boolean" } }
                        },
                        { "$ref": "#/definitions/c" }
                    ],
                    "unevaluatedProperties": false
                },
                "a": {
                    "type": "object",
                    "properties": { "a": { "type": "boolean" } },
                    "unevaluatedProperties": false
                },
                "b": {
                    "type": "object",
                    "properties": { "b": { "type": "boolean" } },
                    "unevaluatedProperties": false
                },
                "c": {
                    "type": "object",
                    "properties": { "c": { "type": "boolean" } }
                }
            },
            "unevaluatedProperties": false
        }));

        assert_schemas_eq(expected_schema, actual_schema);
    }
}
