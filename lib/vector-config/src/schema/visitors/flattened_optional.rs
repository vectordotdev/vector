use serde_json::Value;
use vector_config_common::{
    constants,
    schema::{visit::Visitor, *},
};

use super::merge::Mergeable;

/// Rewrites flattened `Option<T>` subschemas so omission is valid.
///
/// `generate_optional_schema` encodes optionality as `oneOf: [null, T]`, which is correct for a
/// nullable *property*. Flattened fields are merged into the parent object via `allOf`, where the
/// value being validated is never JSON `null`. This visitor replaces that dead null branch with a
/// guard that matches when the flattened value is absent: `not: { required: [<tag field>] }`.
///
/// The rewritten wrapper is promoted from `oneOf` to `anyOf`. Internally tagged enums may include
/// a trailing `#[serde(untagged)]` object or map variant, which also has no tag field; both
/// alternatives then match a serialized `Some(fallback)` value, and `oneOf` would reject it.
/// `anyOf` allows that overlap. Tagged-only enums stay disjoint, so the two keywords are
/// equivalent there.
///
/// The rewrite is scoped to `allOf` members so ordinary `Option<T>` properties are left untouched.
/// Shared `$ref` targets are merged into the flattened site before rewriting (matching
/// `InlineSingleUseReferencesVisitor`), so other usages of the same optional type keep their null
/// branch and the reference site keeps its field-specific metadata.
#[derive(Debug, Default)]
pub struct RewriteFlattenedOptionalVisitor;

impl RewriteFlattenedOptionalVisitor {
    pub fn from_settings(_: &SchemaSettings) -> Self {
        Self
    }
}

impl Visitor for RewriteFlattenedOptionalVisitor {
    fn visit_schema_object(
        &mut self,
        definitions: &mut Map<String, Schema>,
        schema: &mut SchemaObject,
    ) {
        visit::visit_schema_object(self, definitions, schema);

        if let Some(all_of) = schema
            .subschemas
            .as_mut()
            .and_then(|subschemas| subschemas.all_of.as_mut())
        {
            for member in all_of {
                rewrite_flattened_optional_all_of_member(member, definitions);
            }
        }
    }
}

fn rewrite_flattened_optional_all_of_member(
    member: &mut Schema,
    definitions: &Map<String, Schema>,
) {
    if let Schema::Object(schema) = member {
        // Merge a shared optional `$ref` into this `allOf` member so the named definition is
        // unchanged and field-specific metadata on the reference site is preserved.
        if let Some(resolved) = dereference(schema, definitions)
            && is_optional_schema(&resolved)
        {
            schema.reference = None;
            schema.merge(&resolved);
        }

        if is_optional_schema(schema)
            && let Some(tag_field) = enum_tag_field(schema, definitions)
            && replace_null_with_absence(schema, tag_field)
        {
            promote_one_of_to_any_of(schema);
        }
    }
}

fn replace_null_with_absence(schema: &mut SchemaObject, tag_field: String) -> bool {
    if let Some(alternatives) = optional_alternatives_mut(schema)
        && let Some(null_branch) = alternatives
            .iter_mut()
            .find(|schema| is_null_schema(schema))
    {
        *null_branch = Schema::Object(absent_tag_schema(tag_field));
        true
    } else {
        false
    }
}

fn optional_alternatives_mut(schema: &mut SchemaObject) -> Option<&mut Vec<Schema>> {
    schema
        .subschemas
        .as_mut()
        .and_then(|subschemas| subschemas.one_of.as_mut().or(subschemas.any_of.as_mut()))
}

fn promote_one_of_to_any_of(schema: &mut SchemaObject) {
    if let Some(subschemas) = schema.subschemas.as_mut()
        && subschemas.any_of.is_none()
        && let Some(one_of) = subschemas.one_of.take()
    {
        subschemas.any_of = Some(one_of);
    }
}

fn absent_tag_schema(tag_field: String) -> SchemaObject {
    SchemaObject {
        subschemas: Some(Box::new(SubschemaValidation {
            not: Some(Box::new(Schema::Object(SchemaObject {
                object: Some(Box::new(ObjectValidation {
                    required: [tag_field].into_iter().collect(),
                    ..Default::default()
                })),
                ..Default::default()
            }))),
            ..Default::default()
        })),
        ..Default::default()
    }
}

fn enum_tag_field(schema: &SchemaObject, definitions: &Map<String, Schema>) -> Option<String> {
    if let Some(tag) = metadata_str(schema, constants::DOCS_META_ENUM_TAG_FIELD) {
        Some(tag.to_owned())
    } else if let Some(resolved) = dereference(schema, definitions) {
        enum_tag_field(&resolved, definitions)
    } else {
        let subschemas = schema.subschemas.as_ref()?;
        let alternatives = subschemas.one_of.as_ref().or(subschemas.any_of.as_ref())?;
        alternatives
            .iter()
            .filter_map(Schema::as_object)
            .find(|object| !is_null_schema_object(object))
            .and_then(|child| enum_tag_field(child, definitions))
    }
}

fn dereference(schema: &SchemaObject, definitions: &Map<String, Schema>) -> Option<SchemaObject> {
    let reference = schema.reference.as_ref()?;
    match definitions.get(get_cleaned_schema_reference(reference))? {
        Schema::Object(object) => Some(object.clone()),
        Schema::Bool(_) => None,
    }
}

fn is_optional_schema(schema: &SchemaObject) -> bool {
    schema
        .extensions
        .get(constants::METADATA)
        .and_then(|metadata| metadata.get(constants::DOCS_META_OPTIONAL))
        .is_some_and(|value| value.as_bool() == Some(true))
}

fn metadata_str<'a>(schema: &'a SchemaObject, key: &str) -> Option<&'a str> {
    schema
        .extensions
        .get(constants::METADATA)
        .and_then(|metadata| metadata.get(key))
        .and_then(Value::as_str)
}

fn is_null_schema(schema: &Schema) -> bool {
    schema.as_object().is_some_and(is_null_schema_object)
}

fn is_null_schema_object(schema: &SchemaObject) -> bool {
    matches!(
        schema.instance_type.as_ref(),
        Some(SingleOrVec::Single(ty)) if **ty == InstanceType::Null
    )
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use vector_config_common::schema::visit::Visitor;

    use super::RewriteFlattenedOptionalVisitor;
    use crate::schema::visitors::test::{as_schema, assert_schemas_eq};

    /// Generate-path snapshots (`schema_snapshots.rs`) go through `prune_docs`, which
    /// strips `title`/`description`. This fixture drives the visitor directly so those
    /// reference-site fields are still pinned through the `$ref` merge.
    #[test]
    fn merge_preserves_reference_site_metadata() {
        let mut actual_schema = as_schema(json!({
            "definitions": {
                "opt": {
                    "oneOf": [
                        { "type": "null" },
                        { "type": "object" }
                    ],
                    "_metadata": {
                        "docs::optional": true,
                        "docs::enum_tag_field": "type"
                    }
                }
            },
            "allOf": [
                {
                    "$ref": "#/definitions/opt",
                    "title": "Hidden mode",
                    "description": "Not shown in docs.",
                    "deprecated": true
                }
            ]
        }));

        let mut visitor = RewriteFlattenedOptionalVisitor;
        visitor.visit_root_schema(&mut actual_schema);

        let expected_schema = as_schema(json!({
            "definitions": {
                "opt": {
                    "oneOf": [
                        { "type": "null" },
                        { "type": "object" }
                    ],
                    "_metadata": {
                        "docs::optional": true,
                        "docs::enum_tag_field": "type"
                    }
                }
            },
            "allOf": [
                {
                    "title": "Hidden mode",
                    "description": "Not shown in docs.",
                    "deprecated": true,
                    "anyOf": [
                        { "not": { "required": ["type"] } },
                        { "type": "object" }
                    ],
                    "_metadata": {
                        "docs::optional": true,
                        "docs::enum_tag_field": "type"
                    }
                }
            ]
        }));

        assert_schemas_eq(expected_schema, actual_schema);
    }
}
