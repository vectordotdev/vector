use std::collections::HashMap;

use schemars::{gen::SchemaGenerator, schema::{SchemaObject, InstanceType, SingleOrVec}};

use crate::{ArrayShape, Configurable, MapShape, NumberShape, Shape, StringShape, Metadata};

// Null and boolean.
impl<'de, T> Configurable<'de> for Option<T>
where
    T: Configurable<'de>,
{
    fn shape() -> Shape {
        Shape::Optional(Box::new(T::shape()))
    }

	fn generate_schema(gen: &mut SchemaGenerator, overrides: Metadata<Self>) -> SchemaObject {
        // We generate the schema for T itself, and then apply any of T's metadata to the given
        // schema.
		//
		// TODO: we shouldn't realllly need to destructure the metadata to do this, although,
		// admittedly, this is the only place we need to do it so maybe it's not worth tweaking
		// `Metadata` itself, but let's keep our eyes out for a better looking API
		let (inner_overrides_default, inner_overrides_attrs) = overrides.clone().into_parts();
		let inner_overrides = Metadata::new(inner_overrides_default.flatten(), inner_overrides_attrs);
		let mut schema = T::generate_schema(gen, inner_overrides);

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
}

impl<'de> Configurable<'de> for bool {
    fn shape() -> Shape {
        Shape::Boolean
    }

    fn generate_schema(gen: &mut SchemaGenerator, overrides: Metadata<Self>) -> SchemaObject {
        SchemaObject {
			instance_type: Some(InstanceType::Boolean.into()),
			..Default::default()
		}
    }
}

// Strings.
impl<'de> Configurable<'de> for String {
    fn shape() -> Shape {
        Shape::String(StringShape::default())
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
}

impl<'de> Configurable<'de> for f32 {
    fn shape() -> Shape {
        Shape::Number(NumberShape::FloatingPoint {
            minimum: f64::from(f32::MIN),
            maximum: f64::from(f32::MAX),
        })
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
}
