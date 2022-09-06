use std::{
    collections::{BTreeMap, HashMap, HashSet},
    net::SocketAddr,
    num::{
        NonZeroI16, NonZeroI32, NonZeroI64, NonZeroI8, NonZeroU16, NonZeroU32, NonZeroU64,
        NonZeroU8, NonZeroUsize,
    },
    path::PathBuf,
};

use schemars::{gen::SchemaGenerator, schema::SchemaObject};
use serde::Serialize;
use vector_config_common::validation::Validation;

use crate::{
    schema::{
        assert_string_schema_for_map, generate_array_schema, generate_bool_schema,
        generate_map_schema, generate_number_schema, generate_optional_schema, generate_set_schema,
        generate_string_schema,
    },
    str::ConfigurableString,
    Configurable, GenerateError, Metadata,
};

// Unit type.
impl Configurable for () {
    fn generate_schema(_: &mut SchemaGenerator) -> Result<SchemaObject, GenerateError> {
        // Using a unit type in a configuration-related type is never actually valid.
        Err(GenerateError::InvalidType)
    }
}

// Null and boolean.
impl<T> Configurable for Option<T>
where
    T: Configurable + Serialize,
{
    fn is_optional() -> bool {
        true
    }

    fn metadata() -> Metadata<Self> {
        // We clone the default metadata of the wrapped type because otherwise this "level" of the schema would
        // effective sever the link between things like the description of `T` itself and what we show for a field of
        // type `Option<T>`.
        //
        // Said another way, this allows callers to use `#[configurable(derived)]` on a field of `Option<T>` so long as
        // `T` has a description, and both the optional field and the schema for `T` will get the description... but the
        // description for the optional field can still be overridden independently, etc.
        T::metadata().convert()
    }

    fn generate_schema(gen: &mut SchemaGenerator) -> Result<SchemaObject, GenerateError> {
        let mut inner_metadata = T::metadata();
        inner_metadata.set_transparent();

        generate_optional_schema(gen, inner_metadata)
    }
}

impl Configurable for bool {
    fn generate_schema(_: &mut SchemaGenerator) -> Result<SchemaObject, GenerateError> {
        Ok(generate_bool_schema())
    }
}

// Strings.
impl Configurable for String {
    fn generate_schema(_: &mut SchemaGenerator) -> Result<SchemaObject, GenerateError> {
        Ok(generate_string_schema())
    }
}

impl Configurable for char {
    fn metadata() -> Metadata<Self> {
        let mut metadata = Metadata::default();
        if let Some(description) = Self::description() {
            metadata.set_description(description);
        }
        metadata.add_validation(Validation::Length {
            minimum: Some(1),
            maximum: Some(1),
        });
        metadata
    }

    fn generate_schema(_: &mut SchemaGenerator) -> Result<SchemaObject, GenerateError> {
        Ok(generate_string_schema())
    }
}

// Numbers.
macro_rules! impl_configuable_numeric {
	($($ty:ty),+) => {
		$(
			impl Configurable for $ty {
                fn validate_metadata(metadata: &Metadata<Self>) -> Result<(), GenerateError> {
                    $crate::__ensure_numeric_validation_bounds::<Self>(metadata)
                }

				fn generate_schema(_: &mut SchemaGenerator) -> Result<SchemaObject, GenerateError> {
					Ok(generate_number_schema::<Self>())
				}
			}
		)+
	};
}

impl_configuable_numeric!(
    u8,
    u16,
    u32,
    u64,
    usize,
    i8,
    i16,
    i32,
    i64,
    isize,
    f32,
    f64,
    NonZeroU8,
    NonZeroU16,
    NonZeroU32,
    NonZeroU64,
    NonZeroI8,
    NonZeroI16,
    NonZeroI32,
    NonZeroI64,
    NonZeroUsize
);

// Arrays and maps.
impl<T> Configurable for Vec<T>
where
    T: Configurable + Serialize,
{
    fn generate_schema(gen: &mut SchemaGenerator) -> Result<SchemaObject, GenerateError> {
        generate_array_schema::<T>(gen)
    }
}

impl<K, V> Configurable for BTreeMap<K, V>
where
    K: ConfigurableString + Serialize + Ord,
    V: Configurable + Serialize,
{
    fn is_optional() -> bool {
        // A map with required fields would be... an object.  So if you want that, make a struct
        // instead, not a map.
        true
    }

    fn generate_schema(gen: &mut SchemaGenerator) -> Result<SchemaObject, GenerateError> {
        // Make sure our key type is _truly_ a string schema.
        assert_string_schema_for_map::<K, Self>(gen)?;

        generate_map_schema::<V>(gen)
    }
}

impl<K, V> Configurable for HashMap<K, V>
where
    K: ConfigurableString + Serialize + std::hash::Hash + Eq,
    V: Configurable + Serialize,
{
    fn is_optional() -> bool {
        // A map with required fields would be... an object.  So if you want that, make a struct
        // instead, not a map.
        true
    }

    fn generate_schema(gen: &mut SchemaGenerator) -> Result<SchemaObject, GenerateError> {
        // Make sure our key type is _truly_ a string schema.
        assert_string_schema_for_map::<K, Self>(gen)?;

        generate_map_schema::<V>(gen)
    }
}

impl<V> Configurable for HashSet<V>
where
    V: Configurable + Serialize + Eq + std::hash::Hash,
{
    fn generate_schema(gen: &mut SchemaGenerator) -> Result<SchemaObject, GenerateError> {
        generate_set_schema::<V>(gen)
    }
}

// Additional types that do not map directly to scalars.
impl Configurable for SocketAddr {
    fn referenceable_name() -> Option<&'static str> {
        Some("stdlib::SocketAddr")
    }

    fn description() -> Option<&'static str> {
        Some("An internet socket address, either IPv4 or IPv6.")
    }

    fn generate_schema(_: &mut SchemaGenerator) -> Result<SchemaObject, GenerateError> {
        // TODO: We don't need anything other than a string schema to (de)serialize a `SocketAddr`,
        // but we eventually should have validation since the format for the possible permutations
        // is well-known and can be easily codified.
        Ok(generate_string_schema())
    }
}

impl Configurable for PathBuf {
    fn referenceable_name() -> Option<&'static str> {
        Some("stdlib::PathBuf")
    }

    fn description() -> Option<&'static str> {
        Some("A file path.")
    }

    fn metadata() -> Metadata<Self> {
        let mut metadata = Metadata::default();
        if let Some(description) = Self::description() {
            metadata.set_description(description);
        }

        // Taken from
        // https://stackoverflow.com/questions/44289075/regular-expression-to-validate-windows-and-linux-path-with-extension
        // and manually checked against common Linux and Windows paths. It's probably not 100% correct, but it
        // definitely covers the most basic cases.
        const PATH_REGEX: &str = r#"(\/.*|[a-zA-Z]:\\(?:([^<>:"\/\\|?*]*[^<>:"\/\\|?*.]\\|..\\)*([^<>:"\/\\|?*]*[^<>:"\/\\|?*.]\\?|..\\))?)"#;
        metadata.add_validation(Validation::Pattern(PATH_REGEX.to_string()));

        metadata
    }

    fn generate_schema(_: &mut SchemaGenerator) -> Result<SchemaObject, GenerateError> {
        Ok(generate_string_schema())
    }
}
