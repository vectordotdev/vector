use std::collections::BTreeSet;

use indexmap::IndexMap;
use schemars::{
    gen::{SchemaGenerator, SchemaSettings},
    schema::{
        ArrayValidation, InstanceType, ObjectValidation, RootSchema, Schema, SchemaObject,
        SingleOrVec,
    },
};

use crate::{ArrayShape, Configurable, MapShape, Metadata, NumberShape, StringShape};

// TODO: realistically, we should use the availability of the description for a type that
// implements `Configurable` as the basis of whether or not it can be referencable, since without
// that, there's no sane way to promote it to a referencable definition
pub fn finalize_schema<'de, T>(
    gen: &mut SchemaGenerator,
    schema: &mut SchemaObject,
    metadata: Metadata<'de, T>,
) where
    T: Configurable<'de>,
{
    // Figure out if we're dealing with a referencable type or not. A referencable schema
    // can be pointed to and reused over the lifetime of the generation process, so this influences
    // whether or not we even do anything here or not.
    match T::referencable_name() {
        // This type can be referenced, but we don't know if it's actually been defined already or
        // not.  If it has, we alter the caller's schema object to use a reference.
        Some(ref_name) => match gen.definitions().get(ref_name) {
            Some(Schema::Bool(_)) => panic!("all-or-nothing schema definitions are not supported"),
            Some(Schema::Object(ref_schema)) => {
                // We've already finalized a schema for `T` before, so alter the caller's mutable
                // reference to point to it.
                let ref_path = format!("{}{}", gen.settings().definitions_path, ref_name);
                std::mem::replace(schema, SchemaObject::new_ref(ref_path));
            }
            None => {
                // `T` is referencable, but has not been defined yet, so apply the metadata and then
                // define it in the generator.  After that, we alter the caller's schema object to
                // use  a reference.
                apply_metadata(schema, metadata);
                gen.definitions_mut()
                    .insert(ref_name.to_string(), Schema::Object(schema.clone()));
                let ref_path = format!("{}{}", gen.settings().definitions_path, ref_name);
                std::mem::replace(schema, SchemaObject::new_ref(ref_path));
            }
        },
        None => {
            // The type is not referencable, so we just apply the metadata directly and call it a
            // day.
            apply_metadata(schema, metadata);
        }
    }
}

fn apply_metadata<'de, T>(schema: &mut SchemaObject, metadata: Metadata<'de, T>)
where
    T: Configurable<'de>,
{
    // Set the description of this schema.
    let schema_desc = match metadata.description {
        Some(desc) => desc,
        None => {
            panic!("no description provided for `{}`; all `Configurable` types must define a description or be provided one when used within another `Configurable` type", std::any::type_name::<T>());
        }
    };

    // Set the default value for this schema, if any.
    let schema_default = metadata
        .default
        .map(|v| serde_json::to_value(v).expect("default value should never fail to serialize"));

    let schema_metadata = schemars::schema::Metadata {
        description: Some(schema_desc.to_string()),
        default: schema_default,
        ..Default::default()
    };

    schema.metadata = Some(Box::new(schema_metadata));
}

pub fn generate_bool_schema(gen: &mut SchemaGenerator) -> SchemaObject {
    SchemaObject {
        instance_type: Some(InstanceType::Boolean.into()),
        ..Default::default()
    }
}

pub fn generate_string_schema(gen: &mut SchemaGenerator, shape: StringShape) -> SchemaObject {
    SchemaObject {
        instance_type: Some(InstanceType::String.into()),
        string: Some(Box::new(shape.into())),
        ..Default::default()
    }
}

pub fn generate_number_schema(gen: &mut SchemaGenerator, shape: NumberShape) -> SchemaObject {
    SchemaObject {
        instance_type: Some(InstanceType::Number.into()),
        number: Some(Box::new(shape.into())),
        ..Default::default()
    }
}

pub fn generate_array_schema<'de, T>(gen: &mut SchemaGenerator, shape: ArrayShape) -> SchemaObject
where
    T: Configurable<'de>,
{
    // We generate the schema for T itself, and then apply any of T's metadata to the given schema.
    // TODO: fix me and use real metadata
    let element_schema = T::generate_schema(gen, Metadata::default());

    SchemaObject {
        instance_type: Some(InstanceType::Array.into()),
        array: Some(Box::new(ArrayValidation {
            items: Some(SingleOrVec::Single(Box::new(element_schema.into()))),
            min_items: shape.minimum_length,
            max_items: shape.maximum_length,
            ..Default::default()
        })),
        ..Default::default()
    }
}

pub fn generate_map_schema<'de, V>(gen: &mut SchemaGenerator, shape: MapShape) -> SchemaObject
where
    V: Configurable<'de>,
{
    // We generate the schema for V itself, and then apply any of V's metadata to the given schema.
    // TODO: fix me and use real metadata
    let element_schema = V::generate_schema(gen, Metadata::default());

    SchemaObject {
        instance_type: Some(InstanceType::Object.into()),
        object: Some(Box::new(ObjectValidation {
            additional_properties: Some(Box::new(element_schema.into())),
            ..Default::default()
        })),
        ..Default::default()
    }
}

pub fn generate_struct_schema(
    gen: &mut SchemaGenerator,
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

pub fn generate_optional_schema<'de, T>(gen: &mut SchemaGenerator) -> SchemaObject
where
    T: Configurable<'de>,
{
    // We generate the schema for T itself, and then apply any of T's metadata to the given schema.
    // TODO: fix me and use real metadata
    let mut schema = T::generate_schema(gen, Metadata::default());

    // We do a little dance here to add an additional instance type of "null" to the schema to
    // signal it can be "X or null", achieving the functional behavior of "this is optional".
    match schema.instance_type.as_mut() {
        None => panic!("undeclared instance types are not supported"),
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

    schema
}

pub fn generate_composite_schema<'de, T>(gen: &mut SchemaGenerator) -> SchemaObject
where
    T: Configurable<'de>,
{
    SchemaObject::default()
}

pub fn generate_root_schema<'de, T>() -> RootSchema
where
    T: Configurable<'de>,
{
    let schema_settings = SchemaSettings::draft2019_09();
    let mut schema_gen = SchemaGenerator::new(schema_settings);

    let schema = T::generate_schema(&mut schema_gen, Metadata::default());
    RootSchema {
        meta_schema: None,
        schema,
        definitions: schema_gen.take_definitions(),
    }
}
