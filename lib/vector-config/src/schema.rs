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
    let base_metadata = T::metadata();

    // Calculate the title/description of this schema.
    //
    // If the given `metadata` has either a title or description present, we use both those values,
    // even if one of them is `None`. If both are `None`, we try falling back to the base metadata
    // for `T`.
    //
    // This ensures that per-field titles/descriptions can override the base title/description of
    // `T`, without mixing and matching, as sometimes the base type's title/description is far too
    // generic and muddles the output. Essentially, if the callsite decides to provide an overridden
    // title/description, it controls the entire title/description.
    let (schema_title, schema_description) =
        if metadata.title().is_some() || metadata.description().is_some() {
            (metadata.title(), metadata.description())
        } else {
            (base_metadata.title(), base_metadata.description())
        };

    // A description _must_ be present, one way or another, _unless_ one of these two conditions is
    // met:
    // - the field is marked transparent
    // - `T` is referenceable and _does_ have a description
    //
    // We panic otherwise.
    let has_referenceable_description =
        T::referenceable_name().is_some() && base_metadata.description().is_some();
    let is_transparent = base_metadata.transparent() || metadata.transparent();
    if schema_description.is_none() && !is_transparent && !has_referenceable_description {
        let type_name = std::any::type_name::<T>();
        panic!("No description provided for `{}`! All `Configurable` types must define a description, or have one specified at the field-level where the type is being used.", type_name);
    }

    // If a default value was given, serialize it.
    let schema_default = metadata
        .default_value()
        .map(|v| serde_json::to_value(v).expect("default value should never fail to serialize"));

    // Take the existing schema metadata, if any, or create a default version of it, and then apply
    // all of our newly-calculated values to it.
    //
    // Similar to the above title/description logic, we update both title/description if either of
    // them have been set, to avoid mixing/matching between base and override metadata.
    let mut schema_metadata = schema.metadata.take().unwrap_or_default();
    if schema_title.is_some() || schema_description.is_some() {
        schema_metadata.title = schema_title.map(|s| s.to_string());
        schema_metadata.description = schema_description.map(|s| s.to_string());
    }
    schema_metadata.default = schema_default.or(schema_metadata.default);
    schema_metadata.deprecated = metadata.deprecated();

    // Set any custom attributes as extensions on the schema. If an attribute is declared multiple
    // times, we turn the value into an array and merge them together. We _do_ not that, however, if
    // the original value is a flag, or the value being added to an existing key is a flag, as
    // having a flag declared multiple times, or mixing a flag with a KV pair, doesn't make sense.
    let map_entries_len = {
        let custom_map = schema
            .extensions
            .entry("_metadata".to_string())
            .or_insert_with(|| Value::Object(Map::new()))
            .as_object_mut()
            .expect("metadata extension must always be a map");

        if let Some(message) = metadata.deprecated_message() {
            custom_map.insert(
                "deprecated_message".to_string(),
                serde_json::Value::String(message.to_string()),
            );
        }

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

        custom_map.len()
    };

    // If the schema had no existing metadata, and we didn't add any of our own, then remove the
    // metadata extension property entirely, as it would only add noise to the schema output.
    if map_entries_len == 0 {
        schema.extensions.remove("_metadata");
    }

    // Now apply any relevant validations.
    for validation in metadata.validations() {
        validation.apply(schema);
    }

    schema.metadata = Some(schema_metadata);
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
    // Generate the actual schema for the element type `T`.
    let element_schema = get_or_generate_schema::<T>(gen, None)?;

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
    // Generate the actual schema for the element type `T`.
    let element_schema = get_or_generate_schema::<T>(gen, None)?;

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
    // Generate the actual schema for the element type `V`.
    let element_schema = get_or_generate_schema::<V>(gen, None)?;

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

pub fn generate_optional_schema<T>(gen: &mut SchemaGenerator) -> Result<SchemaObject, GenerateError>
where
    T: Configurable + Serialize,
{
    // Optional schemas are generally very simple in practice, but because of how we memoize schema
    // generation and use references to schema definitions, we have to handle quite a few cases
    // here.
    //
    // Specifically, for the `T` in `Option<T>`, we might be dealing with:
    // - a scalar type, where we're going to emit a schema that has `"type": ["string","null"]`, or
    //   something to that effect, where we can simply add the `"`null"` instance type and be done
    // - we may have a referenceable type (i.e. `struct FooBar`) and then we need to generate the
    //   schema for that referenceable type and either:
    //   - append a "null" schema as a `oneOf`/`anyOf` if the generated schema for the referenceable
    //     type already uses that mechanism
    //   - create our own `oneOf` schema to map between either the "null" schema or the real schema

    // Generate the inner schema for `T` We'll add some override metadata, too, so that we can mark
    // this resulting schema as "optional". This is only consequential to documentation generation
    // so that some of the more complex code for parsing enum schemas can correctly differentiate a
    // `oneOf` schema that represents a Rust enum versus one that simply represents our "null or X"
    // wrapped schema.
    let mut overrides = Metadata::default();
    overrides.add_custom_attribute(CustomAttribute::flag("docs::optional"));
    let mut schema = get_or_generate_schema::<T>(gen, Some(overrides))?;

    // Take the metadata and extensions of the original schema.
    //
    // We'll apply these back to `schema` at the end, which will either place them back where they
    // came from (if we don't have to wrap the original schema) or will apply them to the new
    // wrapped schema.
    let original_metadata = schema.metadata.take();
    let original_extensions = std::mem::take(&mut schema.extensions);

    // Figure out if the schema is a referenceable schema or a scalar schema.
    match schema.instance_type.as_mut() {
        // If the schema has no instance types, this implies it's a non-scalar schema: it references
        // another schema, or it's a composite schema/does subschema validation (`$ref`, `oneOf`,
        // `anyOf`, etc).
        //
        // Figure out which it is, and either modify the schema or generate a new schema accordingly.
        None => match schema.subschemas.as_mut() {
            None => {
                // If we don't have a scalar schema, or a schema that uses subschema validation,
                // then we simply create a new schema that uses `oneOf` to allow mapping to either
                // the existing schema _or_ a null schema.
                //
                // This should handle all cases of "normal" referenceable schema types.
                let wrapped_schema = SchemaObject {
                    subschemas: Some(Box::new(SubschemaValidation {
                        one_of: Some(vec![
                            Schema::Object(generate_null_schema()),
                            Schema::Object(std::mem::take(&mut schema)),
                        ]),
                        ..Default::default()
                    })),
                    ..Default::default()
                };

                schema = wrapped_schema;
            }
            Some(subschemas) => {
                if let Some(any_of) = subschemas.any_of.as_mut() {
                    // A null schema is just another possible variant, so we add it directly.
                    any_of.push(Schema::Object(generate_null_schema()));
                } else if let Some(one_of) = subschemas.one_of.as_mut() {
                    // A null schema is just another possible variant, so we add it directly.
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

    // Stick the metadata and extensions back on `schema`.
    schema.metadata = original_metadata;
    schema.extensions = original_extensions;

    Ok(schema)
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

    let schema = get_or_generate_schema::<T>(&mut schema_gen, Some(T::metadata()))?;
    Ok(RootSchema {
        meta_schema: None,
        schema,
        definitions: schema_gen.take_definitions(),
    })
}

pub fn get_or_generate_schema<T>(
    gen: &mut SchemaGenerator,
    overrides: Option<Metadata<T>>,
) -> Result<SchemaObject, GenerateError>
where
    T: Configurable + Serialize,
{
    let (mut schema, metadata) = match T::referenceable_name() {
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

            (get_schema_ref(gen, name), None)
        }
        // Always generate the schema directly if `T` is not referenceable.
        None => (T::generate_schema(gen)?, Some(T::metadata())),
    };

    // Figure out what metadata we should apply to the resulting schema.
    //
    // If `T` was referenceable, we use its implicit metadata when generating the
    // "baseline" schema, because a referenceable type should always be self-contained. We then
    // apply the override metadata, if it exists, to the schema we got back. This allows us to
    // override titles, descriptions, and add additional attributes, and so on.
    //
    // If `T` was not referenceable, we only generate its schema without trying to apply any
    // metadata. We do that because applying schema metadata enforces logic like "can't be without a
    // description". The implicit metadata for `T` may lack that.
    if let Some(overrides) = overrides.as_ref() {
        T::validate_metadata(overrides)?;
    }

    let maybe_metadata = match metadata {
        // If we generated the schema for a referenceable type, we won't need to merge its implicit
        // metadata into the schema we're returning _here_, so just use the override metadata if
        // it was given.
        None => overrides,

        // If we didn't generate the schema for a referenceable type, we'll be holding its implicit
        // metadata here, which we need to merge the override metadata into if it was given. If
        // there was no override metadata, then we just use the base by itself.
        Some(base) => match overrides {
            None => Some(base),
            Some(overrides) => Some(base.merge(overrides)),
        },
    };

    if let Some(metadata) = maybe_metadata {
        apply_metadata(&mut schema, metadata);
    }

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

    let key_schema = get_or_generate_schema::<K>(gen, None)?;
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
