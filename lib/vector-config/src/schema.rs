use std::{collections::BTreeSet, mem};

use indexmap::IndexMap;
use schemars::{
    gen::{SchemaGenerator, SchemaSettings},
    schema::{
        ArrayValidation, InstanceType, NumberValidation, ObjectValidation, RootSchema, Schema,
        SchemaObject, SingleOrVec, SubschemaValidation,
    },
};
use serde::Serialize;
use serde_json::{Map, Value};

use crate::{
    num::ConfigurableNumber, Configurable, ConfigurableString, CustomAttribute, GenerateError,
    Metadata,
};

/// Applies metadata to the given schema.
///
/// Metadata can include semantic information (title, description, etc), validation (min/max, allowable
/// patterns, etc), as well as actual arbitrary key/value data.
pub fn apply_metadata<T>(schema: &mut SchemaObject, metadata: Metadata<T>)
where
    T: Configurable + Serialize,
{
    // Set the title/description of this schema.
    //
    // By default, we want to populate `description` because most things don't need a title: their property name or type
    // name is the title... which is why we enforce description being present at the very least.
    //
    // Additionally, we panic if a description is missing _unless_ one of these two conditions is
    // met:
    // - the field is marked transparent
    // - `T` is referenceable and _does_ have a description
    let has_referenceable_description =
        T::referenceable_name().is_some() && T::metadata().description().is_some();
    let schema_title = metadata.title().map(|s| s.to_string());
    let schema_description = metadata.description().map(|s| s.to_string());
    if schema_description.is_none() && !metadata.transparent() && !has_referenceable_description {
        panic!("no description provided for `{}`; all `Configurable` types must define a description or be provided one when used within another `Configurable` type", std::any::type_name::<T>());
    }

    // Set the default value for this schema, if any.
    let schema_default = metadata
        .default_value()
        .map(|v| serde_json::to_value(v).expect("default value should never fail to serialize"));

    let schema_metadata = schemars::schema::Metadata {
        title: schema_title,
        description: schema_description,
        default: schema_default,
        deprecated: metadata.deprecated(),
        ..Default::default()
    };

    // Set any custom attributes as extensions on the schema. If an attribute is declared multiple
    // times, we turn the value into an array and merge them together. We _do_ not that, however, if
    // the original value is a flag, or the value being added to an existing key is a flag, as
    // having a flag declared multiple times, or mixing a flag with a KV pair, doesn't make sense.
    let mut custom_map = Map::new();
    for attribute in metadata.custom_attributes() {
        match attribute {
            CustomAttribute::Flag(key) => {
                match custom_map.insert(key.to_string(), Value::Bool(true)) {
                    // Overriding a flag is fine, because flags are only ever "enabled", so there's
                    // no harm to enabling it... again. Likewise, if there was no existing value,
                    // it's fine.
                    Some(Value::Bool(_)) | None => {},
                    // Any other value being present means we're clashing with a different metadata
                    // attribute, which is not good, so we have to bail out.
                    _ => panic!("Tried to set metadata flag '{}' but already existed in schema metadata for `{}`.", key, std::any::type_name::<T>()),
                }
            }
            CustomAttribute::KeyValue { key, value } => {
                custom_map.entry(key.to_string())
                    .and_modify(|existing_value| match existing_value {
                        // We already have a flag entry for this key, which we cannot turn into an
                        // array, so we panic in this particular case to signify the weirdness.
                        Value::Bool(_) => {
                            panic!("Tried to overwrite metadata flag '{}' but already existed in schema metadata for `{}` as a flag.", key, std::any::type_name::<T>());
                        },
                        // The entry is already a multi-value KV pair, so just append the value.
                        Value::Array(items) => {
                            items.push(value.clone());
                        },
                        // The entry is not already a multi-value KV pair, so turn it into one.
                        _ => {
                            let taken_existing_value = std::mem::replace(existing_value, Value::Null);
                            *existing_value = Value::Array(vec![taken_existing_value, value.clone()]);
                        },
                    })
                    .or_insert(value.clone());
            }
        }
    }

    if !custom_map.is_empty() {
        schema
            .extensions
            .insert("_metadata".to_string(), Value::Object(custom_map));
    }

    // Now apply any relevant validations.
    for validation in metadata.validations() {
        validation.apply(schema);
    }

    schema.metadata = Some(Box::new(schema_metadata));
}

pub fn convert_to_flattened_schema(primary: &mut SchemaObject, mut subschemas: Vec<SchemaObject>) {
    // First, we replace the primary schema with an empty schema, because we need to push it the actual primary schema
    // into the list of `allOf` schemas. This is due to the fact that it's not valid to "extend" a schema using `allOf`,
    // so everything has to be in there.
    let primary_subschema = mem::take(primary);
    subschemas.insert(0, primary_subschema);

    let all_of_schemas = subschemas.into_iter().map(Schema::Object).collect();

    // Now update the primary schema to use `allOf` to bring everything together.
    primary.subschemas = Some(Box::new(SubschemaValidation {
        all_of: Some(all_of_schemas),
        ..Default::default()
    }));
}

pub fn generate_null_schema() -> SchemaObject {
    SchemaObject {
        instance_type: Some(InstanceType::Null.into()),
        ..Default::default()
    }
}

pub fn generate_bool_schema() -> SchemaObject {
    SchemaObject {
        instance_type: Some(InstanceType::Boolean.into()),
        ..Default::default()
    }
}

pub fn generate_string_schema() -> SchemaObject {
    SchemaObject {
        instance_type: Some(InstanceType::String.into()),
        ..Default::default()
    }
}

pub fn generate_number_schema<N>() -> SchemaObject
where
    N: Configurable + ConfigurableNumber,
{
    // TODO: Once `schemars` has proper integer support, we should allow specifying min/max bounds
    // in a way that's relevant to the number class. As is, we're always forcing bounds to fit into
    // `f64` regardless of whether or not we're using `u64` vs `f64` vs `i16`, and so on.
    let minimum = N::get_enforced_min_bound();
    let maximum = N::get_enforced_max_bound();

    // We always set the minimum/maximum bound to the mechanical limits. Any additional constraining as part of field
    // validators will overwrite these limits.
    let mut schema = SchemaObject {
        instance_type: Some(N::class().as_instance_type().into()),
        number: Some(Box::new(NumberValidation {
            minimum: Some(minimum),
            maximum: Some(maximum),
            ..Default::default()
        })),
        ..Default::default()
    };

    // If the actual numeric type we're generating the schema for is a nonzero variant, and its constraint can't be
    // represented solely by the normal minimum/maximum bounds, we explicitly add an exclusion for the appropriate zero
    // value of the given numeric type.
    if N::requires_nonzero_exclusion() {
        schema.subschemas = Some(Box::new(SubschemaValidation {
            not: Some(Box::new(Schema::Object(SchemaObject {
                const_value: Some(Value::Number(N::get_encoded_zero_value())),
                ..Default::default()
            }))),
            ..Default::default()
        }));
    }

    schema
}

pub fn generate_array_schema<T>(gen: &mut SchemaGenerator) -> Result<SchemaObject, GenerateError>
where
    T: Configurable + Serialize,
{
    // We set `T` to be "transparent", which means that during schema finalization, we will relax
    // the rules we enforce, such as needing a description, knowing that they'll be enforced on the
    // field that is specifying this array schema, since carrying that description forward to `T`
    // would not make sense: if it's a schema reference, the definition will have a description, and
    // otherwise, if it's a primitive like a string... then the field description itself will
    // already inherently describe it.
    let mut metadata = Metadata::<T>::default();
    metadata.set_transparent();

    // Generate the actual schema for the element type `T`.
    let element_schema = get_or_generate_schema::<T>(gen, metadata)?;

    Ok(SchemaObject {
        instance_type: Some(InstanceType::Array.into()),
        array: Some(Box::new(ArrayValidation {
            items: Some(SingleOrVec::Single(Box::new(element_schema.into()))),
            ..Default::default()
        })),
        ..Default::default()
    })
}

pub fn generate_set_schema<T>(gen: &mut SchemaGenerator) -> Result<SchemaObject, GenerateError>
where
    T: Configurable + Serialize,
{
    // We set `T` to be "transparent", which means that during schema finalization, we will relax
    // the rules we enforce, such as needing a description, knowing that they'll be enforced on the
    // field that is specifying this set schema, since carrying that description forward to `T`
    // would not make sense: if it's a schema reference, the definition will have a description, and
    // otherwise, if it's a primitive like a string... then the field description itself will
    // already inherently describe it.
    let mut metadata = Metadata::<T>::default();
    metadata.set_transparent();

    // Generate the actual schema for the element type `T`.
    let element_schema = get_or_generate_schema::<T>(gen, metadata)?;

    Ok(SchemaObject {
        instance_type: Some(InstanceType::Array.into()),
        array: Some(Box::new(ArrayValidation {
            items: Some(SingleOrVec::Single(Box::new(element_schema.into()))),
            unique_items: Some(true),
            ..Default::default()
        })),
        ..Default::default()
    })
}

pub fn generate_map_schema<V>(gen: &mut SchemaGenerator) -> Result<SchemaObject, GenerateError>
where
    V: Configurable + Serialize,
{
    // We set `V` to be "transparent", which means that during schema finalization, we will relax
    // the rules we enforce, such as needing a description, knowing that they'll be enforced on the
    // field that is specifying this map schema, since carrying that description forward to `V`
    // would not make sense: if it's a schema reference, the definition will have a description, and
    // otherwise, if it's a primitive like a string... then the field description itself will
    // already inherently describe it.
    let mut metadata = Metadata::<V>::default();
    metadata.set_transparent();

    // Generate the actual schema for the element type `V`.
    let element_schema = get_or_generate_schema::<V>(gen, metadata)?;

    Ok(SchemaObject {
        instance_type: Some(InstanceType::Object.into()),
        object: Some(Box::new(ObjectValidation {
            additional_properties: Some(Box::new(element_schema.into())),
            ..Default::default()
        })),
        ..Default::default()
    })
}

pub fn generate_struct_schema(
    properties: IndexMap<String, SchemaObject>,
    required: BTreeSet<String>,
    additional_properties: Option<Box<Schema>>,
) -> SchemaObject {
    let properties = properties
        .into_iter()
        .map(|(k, v)| (k, Schema::Object(v)))
        .collect();
    SchemaObject {
        instance_type: Some(InstanceType::Object.into()),
        object: Some(Box::new(ObjectValidation {
            properties,
            required,
            additional_properties,
            ..Default::default()
        })),
        ..Default::default()
    }
}

pub fn make_schema_optional(schema: &mut SchemaObject) -> Result<(), GenerateError> {
    // We do a little dance here to add an additional instance type of "null" to the schema to
    // signal it can be "X or null", achieving the functional behavior of "this is optional".
    match schema.instance_type.as_mut() {
        // If the schema has no instance type, this would generally imply an issue. The one
        // exception to this rule is if the schema represents a composite schema, or in other words,
        // does all validation via a set of subschemas.
        //
        // If we're dealing with one-of or any-of, we insert a subschema that allows for `null`. If
        // we're dealing with all-of, we wrap the existing all-of subschemas in a new schema object,
        // and then change this schema to be a one-of, with two subschemas: a null subschema, and
        // the wrapped all-of subschemas.
        //
        // Anything else like if/then/else... we can't reasonably encode optionality in such a
        // schema, and we have to bail.
        None => match schema.subschemas.as_mut() {
            None => return Err(GenerateError::InvalidOptionalSchema),
            Some(subschemas) => {
                if let Some(any_of) = subschemas.any_of.as_mut() {
                    any_of.push(Schema::Object(generate_null_schema()));
                } else if let Some(one_of) = subschemas.one_of.as_mut() {
                    one_of.push(Schema::Object(generate_null_schema()));
                } else if subschemas.all_of.is_some() {
                    // If we're dealing with an all-of schema, we have to build a new one-of schema
                    // where the two choices are either the `null` schema, or a subschema comprised of
                    // the all-of subschemas.
                    let all_of = subschemas
                        .all_of
                        .take()
                        .expect("all-of subschemas must be present here");
                    let new_all_of_schema = SchemaObject {
                        subschemas: Some(Box::new(SubschemaValidation {
                            all_of: Some(all_of),
                            ..Default::default()
                        })),
                        ..Default::default()
                    };

                    subschemas.one_of = Some(vec![
                        Schema::Object(generate_null_schema()),
                        Schema::Object(new_all_of_schema),
                    ]);
                } else {
                    return Err(GenerateError::InvalidOptionalSchema);
                }
            }
        },
        Some(sov) => match sov {
            SingleOrVec::Single(ty) if **ty != InstanceType::Null => {
                *sov = vec![**ty, InstanceType::Null].into()
            }
            SingleOrVec::Vec(ty) if !ty.contains(&InstanceType::Null) => {
                ty.push(InstanceType::Null)
            }
            _ => {}
        },
    }

    Ok(())
}

pub fn generate_one_of_schema(subschemas: &[SchemaObject]) -> SchemaObject {
    let subschemas = subschemas
        .iter()
        .map(|s| Schema::Object(s.clone()))
        .collect::<Vec<_>>();

    SchemaObject {
        subschemas: Some(Box::new(SubschemaValidation {
            one_of: Some(subschemas),
            ..Default::default()
        })),
        ..Default::default()
    }
}

pub fn generate_all_of_schema(subschemas: &[SchemaObject]) -> SchemaObject {
    let subschemas = subschemas
        .iter()
        .map(|s| Schema::Object(s.clone()))
        .collect::<Vec<_>>();

    SchemaObject {
        subschemas: Some(Box::new(SubschemaValidation {
            all_of: Some(subschemas),
            ..Default::default()
        })),
        ..Default::default()
    }
}

pub fn generate_tuple_schema(subschemas: &[SchemaObject]) -> SchemaObject {
    let subschemas = subschemas
        .iter()
        .map(|s| Schema::Object(s.clone()))
        .collect::<Vec<_>>();

    SchemaObject {
        instance_type: Some(InstanceType::Array.into()),
        array: Some(Box::new(ArrayValidation {
            items: Some(SingleOrVec::Vec(subschemas)),
            // Rust's tuples are closed -- fixed size -- so we set `additionalItems` such that any
            // items past what we have in `items` will cause schema validation to fail.
            additional_items: Some(Box::new(Schema::Bool(false))),
            ..Default::default()
        })),
        ..Default::default()
    }
}

pub fn generate_enum_schema(values: Vec<Value>) -> SchemaObject {
    SchemaObject {
        enum_values: Some(values),
        ..Default::default()
    }
}

pub fn generate_const_string_schema(value: String) -> SchemaObject {
    SchemaObject {
        const_value: Some(Value::String(value)),
        ..Default::default()
    }
}

pub fn generate_internal_tagged_variant_schema(
    tag: String,
    value_schema: SchemaObject,
) -> SchemaObject {
    let mut properties = IndexMap::new();
    properties.insert(tag.clone(), value_schema);

    let mut required = BTreeSet::new();
    required.insert(tag);

    generate_struct_schema(properties, required, None)
}

pub fn generate_root_schema<T>() -> Result<RootSchema, GenerateError>
where
    T: Configurable + Serialize,
{
    // Set env variable to enable generating all schemas, including platform-specific ones.
    std::env::set_var("VECTOR_GENERATE_SCHEMA", "true");

    let mut schema_gen = SchemaSettings::draft2019_09().into_generator();

    let schema = get_or_generate_schema::<T>(&mut schema_gen, T::metadata())?;
    Ok(RootSchema {
        meta_schema: None,
        schema,
        definitions: schema_gen.take_definitions(),
    })
}

pub fn get_or_generate_schema<T>(
    gen: &mut SchemaGenerator,
    overrides: Metadata<T>,
) -> Result<SchemaObject, GenerateError>
where
    T: Configurable + Serialize,
{
    // Ensure the given override metadata is valid for `T`.
    T::validate_metadata(&overrides)?;

    let mut schema = match T::referenceable_name() {
        // When `T` has a referenceable name, try looking it up in the schema generator's definition
        // list, and if it exists, create a schema reference to it. Otherwise, generate it and
        // backfill it in the schema generator.
        Some(name) => {
            if !gen.definitions().contains_key(name) {
                // In order to avoid infinite recursion, we copy the approach that `schemars` takes and
                // insert a dummy boolean schema before actually generating the real schema, and then
                // replace it afterwards. If any recursion occurs, a schema reference will be handed
                // back, which means we don't have to worry about the dummy schema needing to be updated
                // after the fact.
                gen.definitions_mut()
                    .insert(name.to_string(), Schema::Bool(false));

                // We generate the schema for `T` with its own default metadata, and not the
                // override metadata passed into this method, because the override metadata might
                // only be relevant to the place that `T` is being used.
                //
                // For example, if `T` was something for setting the logging level, one component
                // that allows the logging level to be changed for that component specifically might
                // want to specify a default value, whereas `T` should not have a default at all..
                // so if we applied that override metadata, we'd be unwittingly applying a default
                // for all usages of `T` that didn't override the default themselves.
                let schema = generate_baseline_schema::<T>(gen, T::metadata())?;

                gen.definitions_mut()
                    .insert(name.to_string(), Schema::Object(schema));
            }

            get_schema_ref(gen, name)
        }
        // Always generate the schema directly if `T` is not referenceable.
        None => T::generate_schema(gen)?,
    };

    // Apply the overrides metadata to the resulting schema before handing it back.
    //
    // Additionally, following on the comments above about default vs override metadata when
    // generating the schema for `T`, we apply the override metadata here because this is where we
    // would actually be setting the title/description for a field itself, etc, so even if `T` has a
    // title/description, we can specify a more contextual title/description at the point of use.
    apply_metadata(&mut schema, overrides);

    Ok(schema)
}

pub fn generate_baseline_schema<T>(
    gen: &mut SchemaGenerator,
    metadata: Metadata<T>,
) -> Result<SchemaObject, GenerateError>
where
    T: Configurable + Serialize,
{
    // Generate the schema and apply its metadata.
    let mut schema = T::generate_schema(gen)?;
    apply_metadata(&mut schema, metadata);

    Ok(schema)
}

fn get_schema_ref<S: AsRef<str>>(gen: &mut SchemaGenerator, name: S) -> SchemaObject {
    let ref_path = format!("{}{}", gen.settings().definitions_path, name.as_ref());
    SchemaObject::new_ref(ref_path)
}

/// Asserts that the key type `K` generates a string-like schema, suitable for use in maps.
///
/// This function generates a schema for `K` and ensures that the resulting schema is explicitly,
/// but only, represented as a `string` data type. This is necessary to ensure that `K` can be used
/// as the key type for maps, as maps are represented by the `object` data type in JSON Schema,
/// which must have fields with valid string identifiers.
///
/// # Errors
///
/// If the schema is not a valid, string-like schema, an error variant will be returned describing
/// the issue.
pub fn assert_string_schema_for_map<K, M>(gen: &mut SchemaGenerator) -> Result<(), GenerateError>
where
    K: ConfigurableString + Serialize,
{
    // We need to force the schema to be treated as transparent so that when the schema generation
    // finalizes the schema, we don't throw an error due to a lack of title/description.
    let mut key_metadata = Metadata::<K>::default();
    key_metadata.set_transparent();

    let key_schema = get_or_generate_schema::<K>(gen, key_metadata)?;
    let wrapped_schema = Schema::Object(key_schema);

    // Get a reference to the underlying schema if we're dealing with a reference, or just use what
    // we have if it's the actual definition.
    let underlying_schema = if wrapped_schema.is_ref() {
        gen.dereference(&wrapped_schema)
    } else {
        Some(&wrapped_schema)
    };

    let is_string_like = match underlying_schema {
        Some(Schema::Object(schema_object)) => match schema_object.instance_type.as_ref() {
            Some(sov) => match sov {
                // Has to be a string.
                SingleOrVec::Single(it) => **it == InstanceType::String,
                // As long as there's only one instance type, and it's string, we're fine
                // with that, too.
                SingleOrVec::Vec(its) => {
                    its.len() == 1
                        && its
                            .get(0)
                            .filter(|it| *it == &InstanceType::String)
                            .is_some()
                }
            },
            // We match explicitly, so a lack of declared instance types is not considered
            // valid here.
            None => false,
        },
        // We match explicitly, so boolean schemas aren't considered valid here.
        _ => false,
    };

    if !is_string_like {
        Err(GenerateError::MapKeyNotStringLike {
            key_type: std::any::type_name::<K>(),
            map_type: std::any::type_name::<M>(),
        })
    } else {
        Ok(())
    }
}
