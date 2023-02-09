use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    net::SocketAddr,
    num::{
        NonZeroI16, NonZeroI32, NonZeroI64, NonZeroI8, NonZeroU16, NonZeroU32, NonZeroU64,
        NonZeroU8, NonZeroUsize,
    },
    path::PathBuf,
    time::Duration,
};

use indexmap::IndexMap;
use schemars::{gen::SchemaGenerator, schema::SchemaObject};
use serde::Serialize;
use vector_config_common::{attributes::CustomAttribute, validation::Validation};

use crate::{
    num::ConfigurableNumber,
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
    fn referenceable_name() -> Option<&'static str> {
        match T::referenceable_name() {
            None => None,
            Some(_) => Some(std::any::type_name::<Self>()),
        }
    }

    fn is_optional() -> bool {
        true
    }

    fn metadata() -> Metadata<Self> {
        Metadata::with_transparent(true)
    }

    fn validate_metadata(metadata: &Metadata<Self>) -> Result<(), GenerateError> {
        // We have to convert from `Metadata<Self>` to `Metadata<T>` which erases the default value.
        let converted = metadata.convert::<T>();
        T::validate_metadata(&converted)
    }

    fn generate_schema(gen: &mut SchemaGenerator) -> Result<SchemaObject, GenerateError> {
        generate_optional_schema::<T>(gen)
    }
}

impl Configurable for bool {
    fn metadata() -> Metadata<Self> {
        Metadata::with_transparent(true)
    }

    fn generate_schema(_: &mut SchemaGenerator) -> Result<SchemaObject, GenerateError> {
        Ok(generate_bool_schema())
    }
}

// Strings.
impl Configurable for String {
    fn metadata() -> Metadata<Self> {
        Metadata::with_transparent(true)
    }

    fn generate_schema(_: &mut SchemaGenerator) -> Result<SchemaObject, GenerateError> {
        Ok(generate_string_schema())
    }
}

impl Configurable for char {
    fn metadata() -> Metadata<Self> {
        let mut metadata = Metadata::with_transparent(true);
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
macro_rules! impl_configurable_numeric {
	($($ty:ty),+) => {
		$(
			impl Configurable for $ty {
                fn metadata() -> Metadata<Self> {
                    let mut metadata = Metadata::with_transparent(true);
                    let numeric_type = <Self as ConfigurableNumber>::class();
                    metadata.add_custom_attribute(CustomAttribute::kv("docs::numeric_type", numeric_type));

                    metadata
                }

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

impl_configurable_numeric!(
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
    fn metadata() -> Metadata<Self> {
        T::metadata().convert()
    }

    fn validate_metadata(metadata: &Metadata<Self>) -> Result<(), GenerateError> {
        let converted = metadata.convert::<T>();
        T::validate_metadata(&converted)
    }

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

    fn metadata() -> Metadata<Self> {
        Metadata::with_transparent(true)
    }

    fn validate_metadata(metadata: &Metadata<Self>) -> Result<(), GenerateError> {
        let converted = metadata.convert::<V>();
        V::validate_metadata(&converted)
    }

    fn generate_schema(gen: &mut SchemaGenerator) -> Result<SchemaObject, GenerateError> {
        // Make sure our key type is _truly_ a string schema.
        assert_string_schema_for_map::<K, Self>(gen)?;

        generate_map_schema::<V>(gen)
    }
}

impl<V> Configurable for BTreeSet<V>
where
    V: Configurable + Serialize + Eq + std::hash::Hash,
{
    fn metadata() -> Metadata<Self> {
        Metadata::with_transparent(true)
    }

    fn validate_metadata(metadata: &Metadata<Self>) -> Result<(), GenerateError> {
        let converted = metadata.convert::<V>();
        V::validate_metadata(&converted)
    }

    fn generate_schema(gen: &mut SchemaGenerator) -> Result<SchemaObject, GenerateError> {
        generate_set_schema::<V>(gen)
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

    fn metadata() -> Metadata<Self> {
        Metadata::with_transparent(true)
    }

    fn validate_metadata(metadata: &Metadata<Self>) -> Result<(), GenerateError> {
        let converted = metadata.convert::<V>();
        V::validate_metadata(&converted)
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
    fn metadata() -> Metadata<Self> {
        Metadata::with_transparent(true)
    }

    fn validate_metadata(metadata: &Metadata<Self>) -> Result<(), GenerateError> {
        let converted = metadata.convert::<V>();
        V::validate_metadata(&converted)
    }

    fn generate_schema(gen: &mut SchemaGenerator) -> Result<SchemaObject, GenerateError> {
        generate_set_schema::<V>(gen)
    }
}

// Additional types that do not map directly to scalars.
impl Configurable for SocketAddr {
    fn referenceable_name() -> Option<&'static str> {
        Some("stdlib::SocketAddr")
    }

    fn metadata() -> Metadata<Self> {
        let mut metadata = Metadata::default();
        metadata.set_description("An internet socket address, either IPv4 or IPv6.");
        metadata
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

    fn metadata() -> Metadata<Self> {
        let mut metadata = Metadata::default();
        metadata.set_description("A file path.");

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

// The use of `Duration` is deprecated and will be removed in a future version
impl Configurable for Duration {
    fn referenceable_name() -> Option<&'static str> {
        Some("stdlib::Duration")
    }

    fn metadata() -> Metadata<Self> {
        let mut metadata = Metadata::default();
        metadata.set_description("An duration of time.");
        metadata
    }

    fn generate_schema(_: &mut SchemaGenerator) -> Result<SchemaObject, GenerateError> {
        let mut properties = IndexMap::default();
        properties.insert("secs".into(), generate_number_schema::<u64>());
        properties.insert("nsecs".into(), generate_number_schema::<u32>());

        let mut required = BTreeSet::default();
        required.insert("secs".into());
        required.insert("nsecs".into());

        Ok(crate::schema::generate_struct_schema(
            properties, required, None,
        ))
    }
}
