use indexmap::IndexMap;
use vector_config_common::schema::{
    visit::{visit_schema_object, Visitor},
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
#[derive(Debug)]
pub struct DisallowedUnevaluatedPropertiesVisitor {
    definition_path: String,
}

impl DisallowedUnevaluatedPropertiesVisitor {
    pub fn from_settings(settings: &SchemaSettings) -> Self {
        Self {
            definition_path: settings.definitions_path().to_string(),
        }
    }

    fn get_cleaned_schema_ref(&self, schema_ref: &str) -> String {
        if schema_ref.starts_with(&self.definition_path) {
            schema_ref[self.definition_path.len()..].to_string()
        } else {
            schema_ref.to_string()
        }
    }

    fn resolve_schema_reference<'a>(
        &self,
        definitions: &'a IndexMap<String, Schema>,
        schema_ref: &'a str,
    ) -> (String, Schema) {
        let cleaned = self.get_cleaned_schema_ref(schema_ref);
        let resolved = definitions.get(&cleaned).cloned().unwrap_or_else(|| {
            panic!(
                "Unknown schema definition '{}' referenced in schema.",
                cleaned
            )
        });

        (cleaned, resolved)
    }

    fn with_resolved_schema_reference<F>(
        &self,
        definitions: &mut IndexMap<String, Schema>,
        schema: &mut SchemaObject,
        f: F,
    ) where
        F: FnOnce(&mut IndexMap<String, Schema>, &mut SchemaObject),
    {
        if let Some(schema_ref) = schema.reference.as_ref() {
            if let (clean_schema_ref, Schema::Object(mut referenced_schema)) =
                self.resolve_schema_reference(definitions, schema_ref)
            {
                f(definitions, &mut referenced_schema);

                definitions.insert(clean_schema_ref, Schema::Object(referenced_schema));
            }
        }
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

        // If this schema is an object schema (`type` of `object`) or has any subschema validation
        // (`allOf`/`oneOf`/`anyOf`, `if`/`then`/`else`, `$ref`, etc) then we'll set
        // `unevaluatedProperties` to `false`.
        //
        // Crucially, if this schema has any subschema validation and those subschemas have
        // `unevaluatedProperties` set, we will _remove_ it, as subschema validation is
        // fundamentally incompatible in this way: since a subschema is validated against the
        // entirety of the JSON instance passed in at the level of `allOf`, `oneOf`, and so on, each
        // subschema will implicitly be forced to observe other, potentially unrelated properties,
        // and so would naturally fail validation if `unevaluatedProperties` was present in the
        // subschema and set to `false`.

        // First, if this schema itself has a schema reference (`$ref`), we resolve it and visit
        // that resolved schema. The default helper methods don't visit schema references because
        // they don't have enough information to resolve the schema from the definition name.
        //
        // We get an owned and visited version of the resolved schema reference, including its
        // definition name, which we then insert back into `definitions` for subsequent lookups to
        // use the now-updated schema.
        if let Some(schema_ref) = schema.reference.as_ref() {
            if let (clean_schema_ref, Schema::Object(mut referenced_schema)) =
                self.resolve_schema_reference(definitions, schema_ref)
            {
                self.visit_schema_object(definitions, &mut referenced_schema);

                definitions.insert(clean_schema_ref, Schema::Object(referenced_schema));
            }
        }

        // Next, see if this schema has any subschema validation, specifically `allOf` and `oneOf`.
        // If so, we ensure that none of them have `unevaluatedProperties` set at all. We do this
        // because subschema validation involves seeing the entire JSON instance, or seeing a value
        // that's unrelated: we know that some schemas in a `oneOf` won't match, and that's fine,
        // but if they're marked with `unevaluatedProperties: false`, they'll fail... which is why
        // we remove that from the subschemas  themselves but essentially hoist it up to the level
        // of the `allOf`/`oneOf`, where it can ensure the correct behavior.
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
                    object.unevaluated_properties = None;
                } else {
                    self.with_resolved_schema_reference(definitions, subschema, |_, resolved| {
                        if let Some(object) = resolved.object.as_mut() {
                            object.unevaluated_properties = None;
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
    // We only mark the schema as closed if it also does not have `additionalProperties` set to a
    // non-boolean schema. It is a logical inconsistency otherwise.
    if let Some(Schema::Object(_)) = schema
        .object()
        .additional_properties
        .as_ref()
        .map(|v| v.as_ref())
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

    // Grab any subschemas for `allOf` and `oneOf`, if present.
    //
    // There are other subschema validators -- `anyOf` -- as well as other advanced validation
    // mechanisms such as `if`/`then`/`else, but we explicitly don't handle them here as we don't
    // currently use them in Vector's configuration schema.
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
    }

    if validators.is_empty() {
        None
    } else {
        Some(validators)
    }
}
