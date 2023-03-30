//use indexmap::IndexMap;
use vector_config_common::schema::{
    visit::{visit_schema_object, with_resolved_schema_reference, Visitor},
    InstanceType, Map, Schema, SchemaObject, SchemaSettings, SingleOrVec,
};

/// A visitor that marks schemas as disallowing unknown properties via `unevaluatedProperties`.
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
pub struct DisallowedUnevaluatedPropertiesVisitor;

impl DisallowedUnevaluatedPropertiesVisitor {
    pub fn from_settings(_: &SchemaSettings) -> Self {
        Self
    }
}

impl Visitor for DisallowedUnevaluatedPropertiesVisitor {
    fn visit_schema_object(
        &mut self,
        definitions: &mut Map<String, Schema>,
        schema: &mut SchemaObject,
    ) {
        // Visit the schema object first so that we recurse the overall schema in a depth-first
        // fashion, applying the unevaluated properties change from the bottom up.
        visit_schema_object(self, definitions, schema);

        // Next, see if this schema has any subschema validation: `allOf`, `oneOf`, or `anyOf`.
        //
        // If so, we ensure that none of them have `unevaluatedProperties` set at all. We do this
        // because subschema validation involves each subschema seeing the entire JSON instance, or
        // seeing a value that's unrelated: we know that some schemas in a `oneOf` won't match, and
        // that's fine, but if they're marked with `unevaluatedProperties: false`, they'll fail...
        // which is why we remove that from the subschemas themselves but essentially hoist it up
        // to the level of the `allOf`/`oneOf`/`anyOf`, where it can apply the correct behavior.
        let mut had_relevant_subschemas = false;
        if let Some(subschemas) = get_subschema_validators(schema) {
            had_relevant_subschemas = true;

            for subschema in subschemas {
                // If the schema is an object schema, we'll unset `unevaluatedProperties` directly.
                // If it isn't an object schema, we'll see if the subschema is actually a schema
                // reference, and if so, we'll make sure to unset `unevaluatedProperties` on the
                // resolved schema reference itself.
                //
                // Like the top-level schema reference logic, this ensures the schema definition is
                // updated for subsequent resolution.
                if let Some(object) = subschema.object.as_mut() {
                    object.unevaluated_properties = Some(Box::new(Schema::Bool(true)));
                } else {
                    with_resolved_schema_reference(definitions, subschema, |_, resolved| {
                        if let Schema::Object(schema) = resolved {
                            if let Some(object) = schema.object.as_mut() {
                                object.unevaluated_properties = Some(Box::new(Schema::Bool(true)));
                            }
                        }
                    });
                }
            }
        }

        // If we encountered any subschema validation, or if this schema itself is an object schema,
        // mark the schema as closed by setting `unevaluatedProperties` to `false`.
        if had_relevant_subschemas || is_object_schema(schema.instance_type.as_ref()) {
            mark_schema_closed(schema);
        }
    }
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

fn is_object_schema(instance_type: Option<&SingleOrVec<InstanceType>>) -> bool {
    match instance_type {
        Some(sov) => match sov {
            SingleOrVec::Single(inner) => inner.as_ref() == &InstanceType::Object,
            SingleOrVec::Vec(inner) => inner.contains(&InstanceType::Object),
        },
        None => false,
    }
}

fn get_subschema_validators(schema: &mut SchemaObject) -> Option<Vec<&mut SchemaObject>> {
    let mut validators = vec![];

    // Grab any subschemas for `allOf`/`oneOf`/`anyOf`, if present.
    //
    // There are other advanced validation mechanisms such as `if`/`then`/`else, but we explicitly
    // don't handle them here as we don't currently use them in Vector's configuration schema.
    if let Some(subschemas) = schema.subschemas.as_mut() {
        if let Some(all_of) = subschemas.all_of.as_mut() {
            validators.extend(all_of.iter_mut().filter_map(|validator| match validator {
                Schema::Object(inner) => Some(inner),
                _ => None,
            }));
        }

        if let Some(one_of) = subschemas.one_of.as_mut() {
            validators.extend(one_of.iter_mut().filter_map(|validator| match validator {
                Schema::Object(inner) => Some(inner),
                _ => None,
            }));
        }

        if let Some(any_of) = subschemas.any_of.as_mut() {
            validators.extend(any_of.iter_mut().filter_map(|validator| match validator {
                Schema::Object(inner) => Some(inner),
                _ => None,
            }));
        }
    }

    if validators.is_empty() {
        None
    } else {
        Some(validators)
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;
    use serde_json::{json, Value};
    use vector_config_common::schema::{visit::Visitor, RootSchema};

    use super::DisallowedUnevaluatedPropertiesVisitor;

    fn as_schema(value: Value) -> RootSchema {
        serde_json::from_value(value).expect("should not fail to deserialize schema")
    }

    fn assert_schemas_eq(expected: RootSchema, actual: RootSchema) {
        let expected_json = serde_json::to_string_pretty(&expected).expect("should not fail");
        let actual_json = serde_json::to_string_pretty(&actual).expect("should not fail");

        assert_eq!(expected_json, actual_json);
    }

    #[test]
    fn basic_object_schema() {
        let mut actual_schema = as_schema(json!({
            "type": "object",
            "properties": {
                "a": { "type": "string" }
            }
        }));

        let mut visitor = DisallowedUnevaluatedPropertiesVisitor::default();
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

        let mut visitor = DisallowedUnevaluatedPropertiesVisitor::default();
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

        let mut visitor = DisallowedUnevaluatedPropertiesVisitor::default();
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

        let mut visitor = DisallowedUnevaluatedPropertiesVisitor::default();
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

        let mut visitor = DisallowedUnevaluatedPropertiesVisitor::default();
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

        let mut visitor = DisallowedUnevaluatedPropertiesVisitor::default();
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

        let mut visitor = DisallowedUnevaluatedPropertiesVisitor::default();
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

        let mut visitor = DisallowedUnevaluatedPropertiesVisitor::default();
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
}
