#![allow(clippy::borrowed_box)]

use std::mem::discriminant;

use serde_json::Value;
use vector_config_common::schema::*;

/// A type that can be merged with itself.
pub trait Mergeable {
    fn merge(&mut self, other: &Self);
}

impl Mergeable for SchemaObject {
    fn merge(&mut self, other: &Self) {
        // The logic is pretty straightforward: we merge `other` into `self`, with `self` having the
        // higher precedence.
        //
        // Additionally, we only merge logical schema chunks: if the destination schema has object
        // properties defined, and the source schema has some object properties that don't exist in
        // the destination, they will be merged, but if there is any overlap, then the object
        // properties in the destination would remain untouched. This merging logic applies to all
        // map-based types.
        //
        // For standalone fields, such as title or description, the destination always has higher
        // precedence. For optional fields, whichever version (destination or source) is present
        // will win, except for when both are present, then the individual fields within the
        // optional type will be merged according to the normal precedence rules.
        merge_optional(&mut self.reference, other.reference.as_ref());
        merge_schema_metadata(&mut self.metadata, other.metadata.as_ref());
        merge_schema_instance_type(&mut self.instance_type, other.instance_type.as_ref());
        merge_schema_format(&mut self.format, other.format.as_ref());
        merge_schema_enum_values(&mut self.enum_values, other.enum_values.as_ref());
        merge_schema_const_value(&mut self.const_value, other.const_value.as_ref());
        merge_schema_subschemas(&mut self.subschemas, other.subschemas.as_ref());
        merge_schema_number_validation(&mut self.number, other.number.as_ref());
        merge_schema_string_validation(&mut self.string, other.string.as_ref());
        merge_schema_array_validation(&mut self.array, other.array.as_ref());
        merge_schema_object_validation(&mut self.object, other.object.as_ref());
        merge_schema_extensions(&mut self.extensions, &other.extensions);
    }
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
                self_schema.merge(other_schema);
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
    K: Clone + Eq + Ord,
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
        let mut deduped = existing.into_iter().chain(new).cloned().collect::<Vec<_>>();
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
    K: Clone + Eq + Ord,
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
