// TODO: Since we've got the `Mergeable` trait, we might consider going a little more whole hog and
// implementing it for all of the relevant JSON Schema types. We would still need some helper
// functions, but I _think_ it would help clean things up somewhat.
//
// Of course, a theoretical proc macro which derived the trait for us and let us specify merge
// behavior on a per-field basis would make it even _cleaner_, but that's a project for another day.

use std::mem::discriminant;

use indexmap::IndexMap;
use serde_json::Value;
use vector_config_common::schema::{
    visit::{visit_schema_object, Visitor},
    ArrayValidation, InstanceType, Map, Metadata, NumberValidation, ObjectValidation, RootSchema,
    Schema, SchemaObject, SchemaSettings, SingleOrVec, StringValidation, SubschemaValidation,
};

#[derive(Debug, Default)]
pub struct FlattenReferencesAndSubschemasVisitor {
    definition_path: String,
}

impl FlattenReferencesAndSubschemasVisitor {
    pub fn from_settings(settings: &SchemaSettings) -> Self {
        Self {
            definition_path: settings.definitions_path().to_string(),
        }
    }

    fn get_cleaned_schema_ref<'a>(&self, schema_ref: &'a str) -> &'a str {
        if schema_ref.starts_with(&self.definition_path) {
            &schema_ref[self.definition_path.len()..]
        } else {
            schema_ref
        }
    }

    fn resolve_schema_reference<'a>(
        &self,
        definitions: &'a IndexMap<String, Schema>,
        schema_ref: &str,
    ) -> &'a Schema {
        definitions.get(schema_ref).expect(&format!(
            "Unknown schema definition '{}' referenced in schema.",
            schema_ref
        ))
    }
}

impl Visitor for FlattenReferencesAndSubschemasVisitor {
    fn visit_root_schema(&mut self, root: &mut RootSchema) {
        // Visit the root schema object, which will recursively visit the entire schema, flattening
        // any schema references and `allOf` subschemas, in a depth-first fashion.
        self.visit_schema_object(&mut root.definitions, &mut root.schema);

        // After visiting the schema, we'll no longer have any schema references, so we can drop all
        // existing schema definitions.
        root.definitions.clear();
    }

    fn visit_schema_object(
        &mut self,
        definitions: &mut Map<String, Schema>,
        schema: &mut SchemaObject,
    ) {
        // First, we'll handle any direct flattening, which involves either a top-level schema
        // reference (`$ref`) or schema references in an `allOf` subschema validation. We only
        // flatten subschemas in `allOf` here because they're inherently validated at the level of
        // the containing schema.
        //
        // For any schema reference, whether at the top-level or in an `allOf` subschema, the schema
        // being visited has higher precedence during merging.
        if let Some(schema_ref) = schema.reference.take() {
            let schema_ref = self.get_cleaned_schema_ref(&schema_ref);
            let schema_definition = self.resolve_schema_reference(definitions, schema_ref);

            if let Schema::Object(to_merge) = schema_definition {
                let mut to_merge = to_merge.clone();
                self.visit_schema_object(definitions, &mut to_merge);

                merge_schema_objects(schema, &to_merge);

                definitions.insert(schema_ref.to_string(), Schema::Object(to_merge));
            }
        }

        let all_of_subschemas = schema
            .subschemas
            .as_mut()
            .and_then(|subschemas| subschemas.all_of.take());
        if let Some(all_of_subschemas) = all_of_subschemas {
            for subschema in all_of_subschemas {
                if let Schema::Object(mut to_merge) = subschema {
                    self.visit_schema_object(definitions, &mut to_merge);
                    merge_schema_objects(schema, &to_merge);
                }
            }
        }

        // Next, visit any schema-esque portions of the schema object we've been given so that our
        // flattening logic works up from the bottom of the graph.
        //
        // We have to do this after going through the schema reference/`allOf` subschemas to make
        // sure we replace instances that are now newly a part of the given schema object.
        visit_schema_object(self, definitions, schema);
    }
}

fn merge_schema_objects(destination: &mut SchemaObject, source: &SchemaObject) {
    // Since we should always be visiting, and thus flattening, any schemas before they're merged
    // together, the `reference` for both the destination and source schemas should always be empty
    // when this function is called.
    debug_assert!(destination.reference.is_none() && source.reference.is_none());

    // The logic is pretty straightforward: we merge `source` into `destination`, with `destination`
    // having the higher precedence.
    //
    // Additionally, we only merge logical schema chunks: if the destination schema has object
    // properties defined, and the source schema has some object properties that don't exist in the
    // destination, they will be merged, but if there is any overlap, then the object properties in
    // the destination would remain untouched. This merging logic applies to all map-based types.
    //
    // For standalone fields, such as title or description, the destination always has higher
    // precedence. For optional fields, whichever version (destination or source) is present will
    // win, except for when both are present, then the individual fields within the optional type
    // will be merged according to the normal precedence rules.
    merge_schema_metadata(&mut destination.metadata, source.metadata.as_ref());
    merge_schema_instance_type(
        &mut destination.instance_type,
        source.instance_type.as_ref(),
    );
    merge_schema_format(&mut destination.format, source.format.as_ref());
    merge_schema_enum_values(&mut destination.enum_values, source.enum_values.as_ref());
    merge_schema_const_value(&mut destination.const_value, source.const_value.as_ref());
    merge_schema_subschemas(&mut destination.subschemas, source.subschemas.as_ref());
    merge_schema_number_validation(&mut destination.number, source.number.as_ref());
    merge_schema_string_validation(&mut destination.string, source.string.as_ref());
    merge_schema_array_validation(&mut destination.array, source.array.as_ref());
    merge_schema_object_validation(&mut destination.object, source.object.as_ref());
    merge_schema_extensions(&mut destination.extensions, &source.extensions);
}

fn merge_schema_metadata(destination: &mut Option<Box<Metadata>>, source: Option<&Box<Metadata>>) {
    merge_optional_with(destination, source, |existing, new| {
        merge_optional(&mut existing.id, new.id.as_ref());
        merge_optional(&mut existing.title, new.title.as_ref());
        merge_optional(&mut existing.description, new.description.as_ref());
        merge_optional(&mut existing.default, new.default.as_ref());
        merge_bool(&mut existing.deprecated, new.deprecated);
        merge_bool(&mut existing.read_only, new.read_only);
        merge_bool(&mut existing.write_only, new.write_only);
        merge_collection(&mut existing.examples, &new.examples);
    });
}

fn merge_schema_instance_type(
    destination: &mut Option<SingleOrVec<InstanceType>>,
    source: Option<&SingleOrVec<InstanceType>>,
) {
    merge_optional_with(destination, source, |existing, new| {
        let mut deduped = existing
            .into_iter()
            .chain(new.into_iter())
            .cloned()
            .collect::<Vec<_>>();
        deduped.dedup();

        *existing = deduped.into();
    });
}

fn merge_schema_format(destination: &mut Option<String>, source: Option<&String>) {
    merge_optional(destination, source);
}

fn merge_schema_enum_values(destination: &mut Option<Vec<Value>>, source: Option<&Vec<Value>>) {
    merge_optional_with(destination, source, merge_collection);
}

fn merge_schema_const_value(destination: &mut Option<Value>, source: Option<&Value>) {
    merge_optional(destination, source);
}

fn merge_schema_subschemas(
    destination: &mut Option<Box<SubschemaValidation>>,
    source: Option<&Box<SubschemaValidation>>,
) {
    merge_optional_with(destination, source, |existing, new| {
        merge_optional_with(&mut existing.all_of, new.all_of.as_ref(), merge_collection);
        merge_optional_with(&mut existing.any_of, new.any_of.as_ref(), merge_collection);
        merge_optional_with(&mut existing.one_of, new.one_of.as_ref(), merge_collection);
        merge_optional(&mut existing.if_schema, new.if_schema.as_ref());
        merge_optional(&mut existing.then_schema, new.then_schema.as_ref());
        merge_optional(&mut existing.else_schema, new.else_schema.as_ref());
        merge_optional(&mut existing.not, new.not.as_ref());
    });
}

fn merge_schema_number_validation(
    destination: &mut Option<Box<NumberValidation>>,
    source: Option<&Box<NumberValidation>>,
) {
    merge_optional_with(destination, source, |existing, new| {
        merge_optional(&mut existing.multiple_of, new.multiple_of.as_ref());
        merge_optional(&mut existing.maximum, new.maximum.as_ref());
        merge_optional(
            &mut existing.exclusive_maximum,
            new.exclusive_minimum.as_ref(),
        );
        merge_optional(&mut existing.minimum, new.minimum.as_ref());
        merge_optional(
            &mut existing.exclusive_minimum,
            new.exclusive_minimum.as_ref(),
        );
    });
}

fn merge_schema_string_validation(
    destination: &mut Option<Box<StringValidation>>,
    source: Option<&Box<StringValidation>>,
) {
    merge_optional_with(destination, source, |existing, new| {
        merge_optional(&mut existing.max_length, new.max_length.as_ref());
        merge_optional(&mut existing.min_length, new.min_length.as_ref());
        merge_optional(&mut existing.pattern, new.pattern.as_ref());
    });
}

fn merge_schema_array_validation(
    destination: &mut Option<Box<ArrayValidation>>,
    source: Option<&Box<ArrayValidation>>,
) {
    merge_optional_with(destination, source, |existing, new| {
        merge_optional_with(&mut existing.items, new.items.as_ref(), merge_collection);
        merge_optional(
            &mut existing.additional_items,
            new.additional_items.as_ref(),
        );
        merge_optional(
            &mut existing.unevaluated_items,
            new.unevaluated_items.as_ref(),
        );
        merge_optional(&mut existing.max_items, new.max_items.as_ref());
        merge_optional(&mut existing.min_items, new.min_items.as_ref());
        merge_optional(&mut existing.unique_items, new.unique_items.as_ref());
        merge_optional(&mut existing.contains, new.contains.as_ref());
    });
}

fn merge_schema_object_validation(
    destination: &mut Option<Box<ObjectValidation>>,
    source: Option<&Box<ObjectValidation>>,
) {
    merge_optional_with(destination, source, |existing, new| {
        merge_optional(&mut existing.max_properties, new.max_properties.as_ref());
        merge_optional(&mut existing.min_properties, new.min_properties.as_ref());
        merge_collection(&mut existing.required, &new.required);
        merge_map(&mut existing.properties, &new.properties);
        merge_map(&mut existing.pattern_properties, &new.pattern_properties);
        merge_optional(
            &mut existing.additional_properties,
            new.additional_properties.as_ref(),
        );
        merge_optional(
            &mut existing.unevaluated_properties,
            new.unevaluated_properties.as_ref(),
        );
        merge_optional(&mut existing.property_names, new.property_names.as_ref());
    });
}

fn merge_schema_extensions(destination: &mut Map<String, Value>, source: &Map<String, Value>) {
    merge_map(destination, source);
}

fn merge_bool(destination: &mut bool, source: bool) {
    // We only treat `true` as a merge-worthy value.
    if source {
        *destination = true;
    }
}

fn merge_collection<'a, E, I, T>(destination: &mut E, source: I)
where
    E: Extend<T>,
    I: IntoIterator<Item = &'a T>,
    T: Clone + 'a,
{
    destination.extend(source.into_iter().cloned());
}

fn merge_map<K, V>(destination: &mut Map<K, V>, source: &Map<K, V>)
where
    K: std::hash::Hash + Eq + Clone,
    V: Clone + Mergeable,
{
    destination.merge(source);
}

fn merge_optional<T: Clone>(destination: &mut Option<T>, source: Option<&T>) {
    merge_optional_with(destination, source, |_, _| {});
}

fn merge_optional_with<'a, T, F>(destination: &'a mut Option<T>, source: Option<&'a T>, f: F)
where
    T: Clone,
    F: Fn(&'a mut T, &'a T),
{
    match destination {
        // If the destination is empty, we use whatever we have in `source`. Otherwise, we leave
        // `destination` as-is.
        None => *destination = source.cloned(),
        // If `destination` isn't empty, and neither is `source`, then pass them both to `f` to
        // let it handle the actual merge logic.
        Some(destination) => {
            if let Some(source) = source {
                f(destination, source);
            }
        }
    }
}

trait Mergeable {
    fn merge(&mut self, other: &Self);
}

impl Mergeable for Value {
    fn merge(&mut self, other: &Self) {
        // We do a check here for ensuring both value discriminants are the same type. This is
        // specific to `Value` but we should never really be merging identical keys together that
        // have differing value types, as that is indicative of a weird overlap in keys between
        // different schemas.
        //
        // We _may_ need to relax this in practice/in the future, but it's a solid invariant to
        // enforce for the time being.
        if discriminant(self) != discriminant(other) {
            panic!("Tried to merge two `Value` types together with differing types!\n\nSelf: {:?}\n\nOther: {:?}", self, other);
        }

        match (self, other) {
            // Maps get merged recursively.
            (Value::Object(self_map), Value::Object(other_map)) => {
                self_map.merge(other_map);
            }
            // Arrays get merged together indiscriminately.
            (Value::Array(self_array), Value::Array(other_array)) => {
                self_array.extend(other_array.iter().cloned());
            }
            // We don't merge any other value types together.
            _ => {}
        }
    }
}

impl Mergeable for Schema {
    fn merge(&mut self, other: &Self) {
        match (self, other) {
            // We don't merge schemas together if either of them is a boolean schema.
            (Schema::Bool(_), _) | (_, Schema::Bool(_)) => {}
            (Schema::Object(self_schema), Schema::Object(other_schema)) => {
                merge_schema_objects(self_schema, other_schema);
            }
        }
    }
}

impl Mergeable for serde_json::Map<String, Value> {
    fn merge(&mut self, other: &Self) {
        for (key, value) in other {
            match self.get_mut(key) {
                None => {
                    self.insert(key.clone(), value.clone());
                }
                Some(existing) => existing.merge(value),
            }
        }
    }
}

impl<K, V> Mergeable for Map<K, V>
where
    K: std::hash::Hash + Eq + Clone,
    V: Clone + Mergeable,
{
    fn merge(&mut self, other: &Self) {
        for (key, value) in other {
            match self.get_mut(key) {
                None => {
                    self.insert(key.clone(), value.clone());
                }
                Some(existing) => existing.merge(value),
            }
        }
    }
}
