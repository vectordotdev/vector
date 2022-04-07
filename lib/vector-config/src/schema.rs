use std::any::type_name;

use schemars::{JsonSchema, gen::SchemaGenerator, schema::{Schema, SingleOrVec, InstanceType, SchemaObject}};

use crate::{Configurable, Shape, StringShape, NumberShape, ArrayShape, MapShape, Metadata};

trait SealedThingForNow<'de>: Configurable<'de> {}

impl<'de, T> JsonSchema for T
where
	T: SealedThingForNow<'de>
 {
    fn schema_name() -> String {
        T::referencable_name().unwrap_or(type_name::<T>())
    }

    fn json_schema(gen: &mut SchemaGenerator) -> Schema {
        let schema_object = generate_configurable_schema::<Self>(gen);
		Schema::Object(schema_object)
    }

    fn is_referenceable() -> bool {
        T::referencable_name().is_some()
    }
}

fn generate_configurable_schema<'de, T>(gen: &mut SchemaGenerator) -> Schema
where
	T: Configurable<'de>,
{
	let schema = match T::shape() {
		Shape::Boolean => generate_bool_schema::<T>(gen),
		Shape::String(inner) => generate_string_schema::<T>(gen, inner),
		Shape::Number(inner) => generate_number_schema::<T>(gen, inner),
		Shape::Array(inner) => generate_array_schema::<T>(gen, inner),
		Shape::Map(inner) => generate_map_schema::<T>(gen, inner),
		Shape::Optional(inner) => generate_optional_schema::<T>(gen, inner),
		Shape::Composite(inner) => generate_composite_schema::<T>(gen, inner),
	};

	apply_metadata(&mut schema, T::metadata());

	schema
}

fn apply_metadata<'de, T: Configurable<'de>>(schema: &mut SchemaObject, metadata: Metadata<T>) {
    todo!()
}

fn generate_bool_schema<'de, T>(gen: &mut SchemaGenerator) -> SchemaObject
where
	T: Configurable<'de>,
{
	SchemaObject {
		instance_type: Some(SingleOrVec::Single(Box::new(InstanceType::Boolean))),
		..Default::default()
	}
}

fn generate_string_schema<'de, T>(gen: &mut SchemaGenerator, inner: StringShape) -> SchemaObject
where
	T: Configurable<'de>,
{
	let string_validation = inner.into();

	SchemaObject {
		instance_type: Some(SingleOrVec::Single(Box::new(InstanceType::String))),
		string: Some(Box::new(string_validation)),
		..Default::default()
	}
}

fn generate_number_schema<'de, T>(gen: &mut SchemaGenerator, inner: NumberShape) -> SchemaObject
where
	T: Configurable<'de>,
{
	let number_validation = inner.into();

	SchemaObject {
		instance_type: Some(SingleOrVec::Single(Box::new(InstanceType::Number))),
		number: Some(Box::new(number_validation)),
		..Default::default()
	}
}

fn generate_array_schema<'de, T>(gen: &mut SchemaGenerator, inner: ArrayShape) -> SchemaObject
where
	T: Configurable<'de>,
{
}

fn generate_map_schema<'de, T>(gen: &mut SchemaGenerator, inner: MapShape) -> SchemaObject
where
	T: Configurable<'de>,
{
}

fn generate_optional_schema<'de, T>(gen: &mut SchemaGenerator, inner: Box<Shape>) -> SchemaObject
where
	T: Configurable<'de>,
{
	let mut schema = gen.subschema_for::<T>().into_object();
	match schema.instance_type.as_mut() {
		None => panic!("undeclared instance types are not supported"),
		Some(sov) => match sov {
			SingleOrVec::Single(ty) if **ty != InstanceType::Null => {
				*sov = vec![**ty, InstanceType::Null].into()
			}
			SingleOrVec::Vec(ty) if !ty.contains(&InstanceType::Null) => ty.push(InstanceType::Null),
		},
	}
	schema
}


fn generate_composite_schema<'de, T>(gen: &mut SchemaGenerator, inner: Vec<Shape>) -> SchemaObject
where
	T: Configurable<'de>,
{
}
