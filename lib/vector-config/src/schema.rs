use schemars::{
    gen::{SchemaGenerator, SchemaSettings},
    schema::{
        ArrayValidation, InstanceType, ObjectValidation, RootSchema, SchemaObject, SingleOrVec, Metadata as SchemaMetadata,
    },
    JsonSchema,
};

use crate::{ArrayShape, Configurable, Field, Metadata, NumberShape, StringShape, Shape, MapShape};

fn apply_metadata<'de, T>(
    gen: &mut SchemaGenerator,
    schema: &mut SchemaObject,
    metadata: Metadata<T>,
) where
    T: Configurable<'de> + JsonSchema,
{
    // TODO: if the schema object is a reference -- i.e. schema.reference.is_some() -- then we have to mutably borrow its
    // schema definition from the schema generator's definitions to actually update the subschema
    // overall, since otherwise we'd just be updating the ref schema, which won't do what we want.
    //
    // TODO: we probably don't want to update the subschema's metadata multiple times since that's
    // wasteful, but also, maybe it doesn't matter :shrug:
    todo!()
}

pub fn generate_bool_schema(gen: &mut SchemaGenerator) -> SchemaObject {
    let mut schema = SchemaObject {
        instance_type: Some(InstanceType::Boolean.into()),
        ..Default::default()
    };

    schema
}

pub fn generate_string_schema(gen: &mut SchemaGenerator, shape: StringShape) -> SchemaObject {
    let mut schema = SchemaObject {
        instance_type: Some(InstanceType::String.into()),
        string: Some(Box::new(shape.into())),
        ..Default::default()
    };

    schema
}

pub fn generate_number_schema(gen: &mut SchemaGenerator, shape: NumberShape) -> SchemaObject {
    let mut schema = SchemaObject {
        instance_type: Some(InstanceType::Number.into()),
        number: Some(Box::new(shape.into())),
        ..Default::default()
    };

    schema
}

pub fn generate_array_schema<'de, T>(gen: &mut SchemaGenerator, shape: ArrayShape) -> SchemaObject
where
    T: Configurable<'de> + JsonSchema,
{
    // We generate the schema for T itself, and then apply any of T's metadata to the given schema.
    let mut element_schema = gen.subschema_for::<T>().into_object();

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
    V: Configurable<'de> + JsonSchema,
{
    // We generate the schema for V itself, and then apply any of V's metadata to the given schema.
    let mut element_schema = gen.subschema_for::<V>().into_object();

    SchemaObject {
        instance_type: Some(InstanceType::Object.into()),
        object: Some(Box::new(ObjectValidation {
            additional_properties: Some(Box::new(element_schema.into())),
            ..Default::default()
        })),
        ..Default::default()
    }
}

pub fn generate_struct_schema(gen: &mut SchemaGenerator, fields: Vec<Field>, shape: MapShape) -> SchemaObject {
    SchemaObject {
        instance_type: Some(InstanceType::Object.into()),
        object: Some(Box::new(ObjectValidation {
            additional_properties,
            properties,
            ..Default::default()
        })),
        ..Default::default()
    }
}

pub fn generate_optional_schema<'de, T>(gen: &mut SchemaGenerator) -> SchemaObject
where
    T: Configurable<'de> + JsonSchema,
{
    // We generate the schema for T itself, and then apply any of T's metadata to the given schema.
    let mut schema = gen.subschema_for::<T>().into_object();

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
    let schema_settings = SchemaSettings::default();
    let mut schema_gen = SchemaGenerator::new(schema_settings);

    // In order to have a root schema generated, `T` must be an object with named fields, and not a
    // scalar value or array. Practically speaking, a Vector configuration should never be anything
    // _other_ than am object with named fields, so this is mostly a sanity check.
    let fields = T::fields(Metadata::default())
        .expect("root schemas cannot be derived for anything other than structs with named fields");
    let root_ref_name = T::referencable_name()
        .expect("structs with named fields should always have a referencable name");
    let root_description =
        T::description().expect("structs with named fields should always have a description");

    let root_field = Field::referencable::<T>("root", root_ref_name, root_description, Metadata::default());
    let schema = generate_field_schema(&mut schema_gen, root_field);

    RootSchema {
        meta_schema: None,
        schema,
        definitions: schema_gen.take_definitions(),
    }
}

fn generate_field_schema(gen: &mut SchemaGenerator, field: Field) -> SchemaObject {
    // Generate the schema for the field according to its shape. The shape won't always have the
    // necessary information to fully generate the schema, so we sometimes end up passing in other bits.
    let schema = match field.shape {
        Shape::Boolean => generate_bool_schema(gen),
        Shape::String(shape) => generate_string_schema(gen, shape),
        Shape::Number(shape) => generate_number_schema(gen, shape),
        Shape::Array(shape) => generate_array_schema(gen, shape),
        Shape::Map(shape) => if let Some(fields) = field.fields {
            // Since structs with named fields generate their fields via `Configurable::fields`, due
            // to needing override metadata for proper generation, we need to also pass those _in
            // addition_ to the map shape, which dictates some attributes but much more generically.
            generate_struct_schema(gen, fields, shape)
        } else {
            // We just have a normal map -- i.e. HashMap<K, V> -- so we only need the shape to know
            // what constraints to apply to the additional properties.
            generate_map_schema(gen, shape)
        },
        Shape::Optional(_) => todo!(),
        Shape::Composite(_) => todo!(),
    };

    // TODO: other metadata here
    let schema_metadata = SchemaMetadata {
        title: Some(field.name.to_string()),
        description: Some(field.description.to_string()),
        ..Default::default()
    };
    schema.metadata = Some(Box::new(schema_metadata));

    schema
}
