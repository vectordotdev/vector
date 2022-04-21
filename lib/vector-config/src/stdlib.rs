use std::{collections::HashMap, net::SocketAddr};

use schemars::{gen::SchemaGenerator, schema::SchemaObject};

use crate::{
    schema::{
        finalize_schema, generate_array_schema, generate_bool_schema, generate_map_schema,
        generate_number_schema, generate_optional_schema, generate_string_schema,
    },
    ArrayShape, Configurable, Metadata, NumberShape, StringShape,
};

// Unit type.
impl<'de> Configurable<'de> for () {
    fn generate_schema(_: &mut SchemaGenerator, _: Metadata<'de, Self>) -> SchemaObject {
        panic!("unit fields are not supported and should never be used in `Configurable` types");
    }
}

// Null and boolean.
impl<'de, T> Configurable<'de> for Option<T>
where
    T: Configurable<'de>,
{
    fn is_optional() -> bool {
        true
    }

    fn generate_schema(gen: &mut SchemaGenerator, overrides: Metadata<'de, Self>) -> SchemaObject {
        let inner_metadata = overrides.clone().flatten_default();
        let mut schema = generate_optional_schema(gen, inner_metadata);
        finalize_schema(gen, &mut schema, overrides);
        schema
    }
}

impl<'de> Configurable<'de> for bool {
    fn generate_schema(gen: &mut SchemaGenerator, overrides: Metadata<'de, Self>) -> SchemaObject {
        let mut schema = generate_bool_schema();
        finalize_schema(gen, &mut schema, overrides);
        schema
    }
}

// Strings.
impl<'de> Configurable<'de> for String {
    fn generate_schema(gen: &mut SchemaGenerator, overrides: Metadata<'de, Self>) -> SchemaObject {
        // TODO: update shape based on provided metadata
        let shape = StringShape::default();

        let mut schema = generate_string_schema(shape);
        finalize_schema(gen, &mut schema, overrides);
        schema
    }
}

// Numbers.
macro_rules! impl_configuable_unsigned {
	($($ty:ty),+) => {
		$(
			impl<'de> Configurable<'de> for $ty {
				fn generate_schema(gen: &mut SchemaGenerator, overrides: Metadata<'de, Self>) -> SchemaObject {
					// TODO: update shape based on provided metadata
					let shape = NumberShape::unsigned(u64::from(<$ty>::MAX));

					let mut schema = generate_number_schema(shape);
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
				fn generate_schema(gen: &mut SchemaGenerator, overrides: Metadata<'de, Self>) -> SchemaObject {
					// TODO: update shape based on provided metadata
					let shape = NumberShape::signed(i64::from(<$ty>::MIN), i64::from(<$ty>::MAX));

					let mut schema = generate_number_schema(shape);
					finalize_schema(gen, &mut schema, overrides);
					schema
				}
			}
		)+
	};
}

impl_configuable_unsigned!(u8, u16, u32, u64);
impl_configuable_signed!(i8, i16, i32, i64);

impl<'de> Configurable<'de> for usize {
    fn generate_schema(gen: &mut SchemaGenerator, overrides: Metadata<'de, Self>) -> SchemaObject {
        // TODO: update shape based on provided metadata
        let shape = NumberShape::unsigned(u64::try_from(usize::MAX).unwrap_or(u64::MAX));

        let mut schema = generate_number_schema(shape);
        finalize_schema(gen, &mut schema, overrides);
        schema
    }
}

impl<'de> Configurable<'de> for f64 {
    fn generate_schema(gen: &mut SchemaGenerator, overrides: Metadata<'de, Self>) -> SchemaObject {
        // TODO: update shape based on provided metadata
        let shape = NumberShape::FloatingPoint {
            minimum: f64::MIN,
            maximum: f64::MAX,
        };

        let mut schema = generate_number_schema(shape);
        finalize_schema(gen, &mut schema, overrides);
        schema
    }
}

impl<'de> Configurable<'de> for f32 {
    fn generate_schema(gen: &mut SchemaGenerator, overrides: Metadata<'de, Self>) -> SchemaObject {
        // TODO: update shape based on provided metadata
        let shape = NumberShape::FloatingPoint {
            minimum: f64::from(f32::MIN),
            maximum: f64::from(f32::MAX),
        };

        let mut schema = generate_number_schema(shape);
        finalize_schema(gen, &mut schema, overrides);
        schema
    }
}

// Arrays and maps.
impl<'de, T> Configurable<'de> for Vec<T>
where
    T: Configurable<'de>,
{
    fn generate_schema(gen: &mut SchemaGenerator, overrides: Metadata<'de, Self>) -> SchemaObject {
        let element_metadata = T::metadata();
        // TODO: update shape based on provided metadata
        let shape = ArrayShape {
            minimum_length: None,
            maximum_length: None,
        };

        let mut schema = generate_array_schema(gen, shape, element_metadata);
        finalize_schema(gen, &mut schema, overrides);
        schema
    }
}

impl<'de, V> Configurable<'de> for HashMap<String, V>
where
    V: Configurable<'de>,
{
    fn is_optional() -> bool {
        // A hashmap with required fields would be... an object.  So if you want that, make a struct
        // instead, not a hashmap.
        true
    }

    fn generate_schema(gen: &mut SchemaGenerator, overrides: Metadata<'de, Self>) -> SchemaObject {
        // We explicitly do not pass anything from the override metadata, because there's nothing to
        // reasonably pass: if `V` is referencable, using the description for `HashMap<String, V>`
        // likely makes no sense, nor would a default make sense, and so on.
        //
        // We do, however, set `V` to be "transparent", which means that during schema finalization,
        // we will relax the rules we enforce, such as needing a description, knowing that they'll
        // be enforced on the field using `HashMap<String, V>` itself, where carrying that
        // description forward to `V` might literally make no sense, such as when `V` is a primitive
        // type like an integer or string.
        let mut value_metadata = V::metadata();
        value_metadata.set_transparent();

        let mut schema = generate_map_schema(gen, value_metadata);
        finalize_schema(gen, &mut schema, overrides);
        schema
    }
}

impl<'de> Configurable<'de> for SocketAddr {
    fn referencable_name() -> Option<&'static str> {
        Some("SocketAddr")
    }

    fn description() -> Option<&'static str> {
        Some("An internet socket address, either IPv4 or IPv6.")
    }

    fn generate_schema(gen: &mut SchemaGenerator, overrides: Metadata<'de, Self>) -> SchemaObject {
        // TODO: We don't need anything other than a string schema to (de)serialize a `SocketAddr`,
        // but we eventually should have validation since the format for the possible permutations
        // is well-known and can be easily codified.
        let shape = StringShape::default();

        let mut schema = generate_string_schema(shape);
        finalize_schema(gen, &mut schema, overrides);
        schema
    }
}
