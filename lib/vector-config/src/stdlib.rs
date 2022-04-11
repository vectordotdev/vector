use std::collections::HashMap;

use schemars::{
    gen::SchemaGenerator,
    schema::{ArrayValidation, InstanceType, ObjectValidation, SchemaObject, SingleOrVec},
};

use crate::{
    schema::finalize_schema, ArrayShape, Configurable, MapShape, Metadata, NumberShape, Shape,
    StringShape,
};

// Null and boolean.
impl<'de, T> Configurable<'de> for Option<T>
where
    T: Configurable<'de>,
{
    fn shape() -> Shape {
        Shape::Optional(Box::new(T::shape()))
    }

    fn generate_schema(gen: &mut SchemaGenerator, overrides: Metadata<'de, Self>) -> SchemaObject {
        // We generate the schema for T itself, and then apply any of T's metadata to the given
        // schema.
        let (inner_metadata_desc, inner_metadata_default, inner_metadata_attrs) =
            overrides.clone().into_parts();
        let inner_metadata = Metadata::new(
            inner_metadata_desc,
            inner_metadata_default.flatten(),
            inner_metadata_attrs,
        );
        let mut schema = T::generate_schema(gen, inner_metadata);

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

        finalize_schema(gen, &mut schema, overrides);

        schema
    }
}

impl<'de> Configurable<'de> for bool {
    fn shape() -> Shape {
        Shape::Boolean
    }

    fn generate_schema(gen: &mut SchemaGenerator, overrides: Metadata<'de, Self>) -> SchemaObject {
        let mut schema = SchemaObject {
            instance_type: Some(InstanceType::Boolean.into()),
            ..Default::default()
        };

        finalize_schema(gen, &mut schema, overrides);
        schema
    }
}

// Strings.
impl<'de> Configurable<'de> for String {
    fn shape() -> Shape {
        Shape::String(StringShape::default())
    }

    fn generate_schema(gen: &mut SchemaGenerator, overrides: Metadata<'de, Self>) -> SchemaObject {
        // TODO: update shape based on provided metadata
        let final_shape = StringShape::default();

        let mut schema = SchemaObject {
            instance_type: Some(InstanceType::String.into()),
            string: Some(Box::new(final_shape.into())),
            ..Default::default()
        };

        finalize_schema(gen, &mut schema, overrides);
        schema
    }
}

// Numbers.
macro_rules! impl_configuable_unsigned {
	($($ty:ty),+) => {
		$(
			impl<'de> Configurable<'de> for $ty {
				fn shape() -> Shape {
					Shape::Number(NumberShape::Unsigned {
						minimum: u64::from(<$ty>::MIN),
						maximum: u64::from(<$ty>::MAX),
					})
				}

				fn generate_schema(gen: &mut SchemaGenerator, overrides: Metadata<'de, Self>) -> SchemaObject {
					// TODO: update shape based on provided metadata
					let final_shape = NumberShape::unsigned(u64::from(<$ty>::MAX));

					let mut schema = SchemaObject {
						instance_type: Some(InstanceType::Number.into()),
						number: Some(Box::new(final_shape.into())),
						..Default::default()
					};

					finalize_schema(gen, &mut schema, overrides);
					schema
				}
			}
		)+
	};
}

macro_rules! impl_configuable_signed {
	($($ty:ty),+) => {
		$(
			impl<'de> Configurable<'de> for $ty {
				fn shape() -> Shape {
					Shape::Number(NumberShape::Signed {
						minimum: i64::from(<$ty>::MIN),
						maximum: i64::from(<$ty>::MAX),
					})
				}

				fn generate_schema(gen: &mut SchemaGenerator, overrides: Metadata<'de, Self>) -> SchemaObject {
					// TODO: update shape based on provided metadata
					let final_shape = NumberShape::signed(i64::from(<$ty>::MIN), i64::from(<$ty>::MAX));

					let mut schema = SchemaObject {
						instance_type: Some(InstanceType::Number.into()),
						number: Some(Box::new(final_shape.into())),
						..Default::default()
					};

					finalize_schema(gen, &mut schema, overrides);
					schema
				}
			}
		)+
	};
}

impl_configuable_unsigned!(u8, u16, u32, u64);
impl_configuable_signed!(i8, i16, i32, i64);

impl<'de> Configurable<'de> for f64 {
    fn shape() -> Shape {
        Shape::Number(NumberShape::FloatingPoint {
            minimum: f64::MIN,
            maximum: f64::MAX,
        })
    }

    fn generate_schema(gen: &mut SchemaGenerator, overrides: Metadata<'de, Self>) -> SchemaObject {
        // TODO: update shape based on provided metadata
        let final_shape = NumberShape::FloatingPoint {
            minimum: f64::MIN,
            maximum: f64::MAX,
        };

        let mut schema = SchemaObject {
            instance_type: Some(InstanceType::Number.into()),
            number: Some(Box::new(final_shape.into())),
            ..Default::default()
        };

        finalize_schema(gen, &mut schema, overrides);
        schema
    }
}

impl<'de> Configurable<'de> for f32 {
    fn shape() -> Shape {
        Shape::Number(NumberShape::FloatingPoint {
            minimum: f64::from(f32::MIN),
            maximum: f64::from(f32::MAX),
        })
    }

    fn generate_schema(gen: &mut SchemaGenerator, overrides: Metadata<'de, Self>) -> SchemaObject {
        // TODO: update shape based on provided metadata
        let final_shape = NumberShape::FloatingPoint {
            minimum: f64::from(f32::MIN),
            maximum: f64::from(f32::MAX),
        };

        let mut schema = SchemaObject {
            instance_type: Some(InstanceType::Number.into()),
            number: Some(Box::new(final_shape.into())),
            ..Default::default()
        };

        finalize_schema(gen, &mut schema, overrides);
        schema
    }
}

// Arrays and maps.
impl<'de, T> Configurable<'de> for Vec<T>
where
    T: Configurable<'de>,
{
    fn shape() -> Shape {
        Shape::Array(ArrayShape {
            element_shape: Box::new(T::shape()),
            minimum_length: None,
            maximum_length: None,
        })
    }

    fn generate_schema(gen: &mut SchemaGenerator, overrides: Metadata<'de, Self>) -> SchemaObject {
        // We generate the schema for T itself, and then apply any of T's metadata to the given
        // schema. We explicitly do not pass down a default, if we have one, because given that it
        // might be specific to this field, and that there might be multiple of them, the default
        // given _here_ could only apply to `Vec<T>`, and couldn't be destructed to pass down to `T`.
        let (element_metadata_desc, _, element_metadata_attrs) = overrides.clone().into_parts();
        let element_metadata = Metadata::new(element_metadata_desc, None, element_metadata_attrs);
        let mut element_schema = T::generate_schema(gen, element_metadata);

        // TODO: update shape based on provided metadata
        let final_shape = ArrayShape {
            element_shape: Box::new(T::shape()),
            minimum_length: None,
            maximum_length: None,
        };

        let mut schema = SchemaObject {
            instance_type: Some(InstanceType::Array.into()),
            array: Some(Box::new(ArrayValidation {
                items: Some(SingleOrVec::Single(Box::new(element_schema.into()))),
                min_items: final_shape.minimum_length,
                max_items: final_shape.maximum_length,
                ..Default::default()
            })),
            ..Default::default()
        };

        finalize_schema(gen, &mut schema, overrides);
        schema
    }
}

impl<'de, V> Configurable<'de> for HashMap<String, V>
where
    V: Configurable<'de>,
{
    fn shape() -> Shape {
        Shape::Map(MapShape {
            required_fields: HashMap::new(),
            allowed_unknown_field_shape: Some(Box::new(V::shape())),
        })
    }

    fn generate_schema(gen: &mut SchemaGenerator, overrides: Metadata<'de, Self>) -> SchemaObject {
        // We generate the schema for T itself, and then apply any of T's metadata to the given
        // schema. We explicitly do not pass down a default, if we have one, because given that it
        // might be specific to this field, and that there might be multiple of them, the default
        // given _here_ could only apply to `HashMap<String, V>`, and couldn't be destructed to pass
        // down to `V`.

        // TODO: This ends up looking kind of weird/annoying because we're carrying over the
        // description of the field itself -- i.e. whatever the doc comment is for the thing using
        // HashMap<String, V> -- and using _that_ for the description of `V`, which is duplicative
        // since we really want it at the field level and nowhere else below.
        //
        // It's made even tougher because if we pass in an empty string override, we'd be messing
        // things up when `V` is a complex type.  Essentially, we lack the necessary control to
        // inform the finalization step that the need for a description is contingent on it not
        // being a referencable type, since that'd be the only case where we want a description to
        // be used for `V`, as it would exist in the definition and not at the callsite.
        let (value_metadata_desc, _, value_metadata_attrs) = overrides.clone().into_parts();
        let value_metadata = Metadata::new(value_metadata_desc, None, value_metadata_attrs);
        let value_schema = V::generate_schema(gen, value_metadata);

        let mut schema = SchemaObject {
            instance_type: Some(InstanceType::Object.into()),
            object: Some(Box::new(ObjectValidation {
                additional_properties: Some(Box::new(value_schema.into())),
                ..Default::default()
            })),
            ..Default::default()
        };

        finalize_schema(gen, &mut schema, overrides);
        schema
    }
}
