use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    hash::Hash,
    net::SocketAddr,
    num::{
        NonZeroI16, NonZeroI32, NonZeroI64, NonZeroI8, NonZeroU16, NonZeroU32, NonZeroU64,
        NonZeroU8, NonZeroUsize,
    },
    path::PathBuf,
    time::Duration,
};

use indexmap::IndexMap;
use serde_json::{Number, Value};
use vector_config_common::{attributes::CustomAttribute, constants, validation::Validation};
use vrl::value::KeyString;

use crate::{
    num::ConfigurableNumber,
    schema::{
        assert_string_schema_for_map, generate_array_schema, generate_bool_schema,
        generate_map_schema, generate_number_schema, generate_optional_schema, generate_set_schema,
        generate_string_schema, SchemaGenerator, SchemaObject,
    },
    str::ConfigurableString,
    Configurable, GenerateError, Metadata, ToValue,
};

// Unit type.
impl Configurable for () {
    fn generate_schema(_: &RefCell<SchemaGenerator>) -> Result<SchemaObject, GenerateError> {
        // Using a unit type in a configuration-related type is never actually valid.
        Err(GenerateError::InvalidType)
    }
}

impl ToValue for () {
    fn to_value(&self) -> Value {
        Value::Null
    }
}

// Null and boolean.
impl<T> Configurable for Option<T>
where
    T: Configurable + ToValue + 'static,
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

    fn metadata() -> Metadata {
        Metadata::with_transparent(true)
    }

    fn validate_metadata(metadata: &Metadata) -> Result<(), GenerateError> {
        // We have to convert from `Metadata` to `Metadata` which erases the default value.
        let converted = metadata.convert();
        T::validate_metadata(&converted)
    }

    fn generate_schema(gen: &RefCell<SchemaGenerator>) -> Result<SchemaObject, GenerateError> {
        generate_optional_schema(&T::as_configurable_ref(), gen)
    }
}

impl<T: ToValue> ToValue for Option<T> {
    fn to_value(&self) -> Value {
        match self {
            None => Value::Null,
            Some(inner) => inner.to_value(),
        }
    }
}

impl Configurable for bool {
    fn metadata() -> Metadata {
        Metadata::with_transparent(true)
    }

    fn generate_schema(_: &RefCell<SchemaGenerator>) -> Result<SchemaObject, GenerateError> {
        Ok(generate_bool_schema())
    }
}

impl ToValue for bool {
    fn to_value(&self) -> Value {
        Value::Bool(*self)
    }
}

// Strings.
impl Configurable for String {
    fn metadata() -> Metadata {
        Metadata::with_transparent(true)
    }

    fn generate_schema(_: &RefCell<SchemaGenerator>) -> Result<SchemaObject, GenerateError> {
        Ok(generate_string_schema())
    }
}

impl ToValue for String {
    fn to_value(&self) -> Value {
        Value::String(self.clone())
    }
}

impl Configurable for KeyString {
    fn metadata() -> Metadata {
        Metadata::with_transparent(true)
    }

    fn generate_schema(_: &RefCell<SchemaGenerator>) -> Result<SchemaObject, GenerateError> {
        Ok(generate_string_schema())
    }
}

impl ToValue for KeyString {
    fn to_value(&self) -> Value {
        Value::String(self.clone().into())
    }
}

impl Configurable for char {
    fn metadata() -> Metadata {
        let mut metadata = Metadata::with_transparent(true);
        metadata.add_validation(Validation::Length {
            minimum: Some(1),
            maximum: Some(1),
        });
        metadata
    }

    fn generate_schema(_: &RefCell<SchemaGenerator>) -> Result<SchemaObject, GenerateError> {
        Ok(generate_string_schema())
    }
}

impl ToValue for char {
    fn to_value(&self) -> Value {
        Value::String(format!("{self}"))
    }
}

// Numbers.
macro_rules! impl_configurable_numeric {
    ($ty:ty => $into:expr) => {
        impl Configurable for $ty {
            fn metadata() -> Metadata {
                let mut metadata = Metadata::with_transparent(true);
                let numeric_type = <Self as ConfigurableNumber>::class();
                metadata.add_custom_attribute(CustomAttribute::kv(
                    constants::DOCS_META_NUMERIC_TYPE,
                    numeric_type,
                ));

                metadata
            }

            fn validate_metadata(metadata: &Metadata) -> Result<(), GenerateError> {
                $crate::__ensure_numeric_validation_bounds::<Self>(metadata)
            }

            fn generate_schema(
                _: &RefCell<SchemaGenerator>,
            ) -> Result<SchemaObject, GenerateError> {
                Ok(generate_number_schema::<Self>())
            }
        }

        impl ToValue for $ty {
            fn to_value(&self) -> Value {
                let into = $into;
                Value::Number(into(*self))
            }
        }
    };
}

impl_configurable_numeric!(u8 => Into::into);
impl_configurable_numeric!(u16 => Into::into);
impl_configurable_numeric!(u32 => Into::into);
impl_configurable_numeric!(u64 => Into::into);
impl_configurable_numeric!(usize => Into::into);
impl_configurable_numeric!(i8 => Into::into);
impl_configurable_numeric!(i16 => Into::into);
impl_configurable_numeric!(i32 => Into::into);
impl_configurable_numeric!(i64 => Into::into);
impl_configurable_numeric!(isize => Into::into);
impl_configurable_numeric!(f32 => |v| Number::from_f64(v as f64).expect("Could not convert number to JSON"));
impl_configurable_numeric!(f64 => |v| Number::from_f64(v).expect("Could not convert number to JSON"));
impl_configurable_numeric!(NonZeroU8 => |v: NonZeroU8| v.get().into());
impl_configurable_numeric!(NonZeroU16 => |v: NonZeroU16| v.get().into());
impl_configurable_numeric!(NonZeroU32 => |v: NonZeroU32| v.get().into());
impl_configurable_numeric!(NonZeroU64 => |v: NonZeroU64| v.get().into());
impl_configurable_numeric!(NonZeroI8 => |v: NonZeroI8| v.get().into());
impl_configurable_numeric!(NonZeroI16 => |v: NonZeroI16| v.get().into());
impl_configurable_numeric!(NonZeroI32 => |v: NonZeroI32| v.get().into());
impl_configurable_numeric!(NonZeroI64 => |v: NonZeroI64| v.get().into());
impl_configurable_numeric!(NonZeroUsize => |v: NonZeroUsize| v.get().into());

// Arrays and maps.
impl<T> Configurable for Vec<T>
where
    T: Configurable + ToValue + 'static,
{
    fn metadata() -> Metadata {
        T::metadata().convert()
    }

    fn validate_metadata(metadata: &Metadata) -> Result<(), GenerateError> {
        let converted = metadata.convert();
        T::validate_metadata(&converted)
    }

    fn generate_schema(gen: &RefCell<SchemaGenerator>) -> Result<SchemaObject, GenerateError> {
        generate_array_schema(&T::as_configurable_ref(), gen)
    }
}

impl<T: ToValue> ToValue for Vec<T> {
    fn to_value(&self) -> Value {
        Value::Array(self.iter().map(ToValue::to_value).collect())
    }
}

impl<K, V> Configurable for BTreeMap<K, V>
where
    K: ConfigurableString + Ord + ToValue + 'static,
    V: Configurable + ToValue + 'static,
{
    fn is_optional() -> bool {
        // A map with required fields would be... an object.  So if you want that, make a struct
        // instead, not a map.
        true
    }

    fn metadata() -> Metadata {
        Metadata::with_transparent(true)
    }

    fn validate_metadata(metadata: &Metadata) -> Result<(), GenerateError> {
        let converted = metadata.convert();
        V::validate_metadata(&converted)
    }

    fn generate_schema(gen: &RefCell<SchemaGenerator>) -> Result<SchemaObject, GenerateError> {
        // Make sure our key type is _truly_ a string schema.
        assert_string_schema_for_map(
            &K::as_configurable_ref(),
            gen,
            std::any::type_name::<Self>(),
        )?;

        generate_map_schema(&V::as_configurable_ref(), gen)
    }
}

impl<K, V> ToValue for BTreeMap<K, V>
where
    K: ToString,
    V: ToValue,
{
    fn to_value(&self) -> Value {
        Value::Object(
            self.iter()
                .map(|(k, v)| (k.to_string(), v.to_value()))
                .collect(),
        )
    }
}

impl<V> Configurable for BTreeSet<V>
where
    V: Configurable + ToValue + Eq + Hash + 'static,
{
    fn metadata() -> Metadata {
        Metadata::with_transparent(true)
    }

    fn validate_metadata(metadata: &Metadata) -> Result<(), GenerateError> {
        let converted = metadata.convert();
        V::validate_metadata(&converted)
    }

    fn generate_schema(gen: &RefCell<SchemaGenerator>) -> Result<SchemaObject, GenerateError> {
        generate_set_schema(&V::as_configurable_ref(), gen)
    }
}

impl<V: ToValue> ToValue for BTreeSet<V> {
    fn to_value(&self) -> Value {
        Value::Array(self.iter().map(ToValue::to_value).collect())
    }
}

impl<K, V> Configurable for HashMap<K, V>
where
    K: ConfigurableString + ToValue + Hash + Eq + 'static,
    V: Configurable + ToValue + 'static,
{
    fn is_optional() -> bool {
        // A map with required fields would be... an object.  So if you want that, make a struct
        // instead, not a map.
        true
    }

    fn metadata() -> Metadata {
        Metadata::with_transparent(true)
    }

    fn validate_metadata(metadata: &Metadata) -> Result<(), GenerateError> {
        let converted = metadata.convert();
        V::validate_metadata(&converted)
    }

    fn generate_schema(gen: &RefCell<SchemaGenerator>) -> Result<SchemaObject, GenerateError> {
        // Make sure our key type is _truly_ a string schema.
        assert_string_schema_for_map(
            &K::as_configurable_ref(),
            gen,
            std::any::type_name::<Self>(),
        )?;

        generate_map_schema(&V::as_configurable_ref(), gen)
    }
}

impl<K, V> ToValue for HashMap<K, V>
where
    K: ToString,
    V: ToValue,
{
    fn to_value(&self) -> Value {
        Value::Object(
            self.iter()
                .map(|(k, v)| (k.to_string(), v.to_value()))
                .collect(),
        )
    }
}

impl<V> Configurable for HashSet<V>
where
    V: Configurable + ToValue + Eq + Hash + 'static,
{
    fn metadata() -> Metadata {
        Metadata::with_transparent(true)
    }

    fn validate_metadata(metadata: &Metadata) -> Result<(), GenerateError> {
        let converted = metadata.convert();
        V::validate_metadata(&converted)
    }

    fn generate_schema(gen: &RefCell<SchemaGenerator>) -> Result<SchemaObject, GenerateError> {
        generate_set_schema(&V::as_configurable_ref(), gen)
    }
}

impl<V> ToValue for HashSet<V>
where
    V: ToValue,
{
    fn to_value(&self) -> Value {
        Value::Array(self.iter().map(ToValue::to_value).collect())
    }
}

// Additional types that do not map directly to scalars.
impl Configurable for SocketAddr {
    fn referenceable_name() -> Option<&'static str> {
        Some("stdlib::SocketAddr")
    }

    fn metadata() -> Metadata {
        let mut metadata = Metadata::default();
        metadata.set_description("An internet socket address, either IPv4 or IPv6.");
        metadata
    }

    fn generate_schema(_: &RefCell<SchemaGenerator>) -> Result<SchemaObject, GenerateError> {
        // TODO: We don't need anything other than a string schema to (de)serialize a `SocketAddr`,
        // but we eventually should have validation since the format for the possible permutations
        // is well-known and can be easily codified.
        Ok(generate_string_schema())
    }
}

impl ToValue for SocketAddr {
    fn to_value(&self) -> Value {
        Value::String(self.to_string())
    }
}

impl Configurable for PathBuf {
    fn referenceable_name() -> Option<&'static str> {
        Some("stdlib::PathBuf")
    }

    fn metadata() -> Metadata {
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

    fn generate_schema(_: &RefCell<SchemaGenerator>) -> Result<SchemaObject, GenerateError> {
        Ok(generate_string_schema())
    }
}

impl ToValue for PathBuf {
    fn to_value(&self) -> Value {
        Value::String(self.display().to_string())
    }
}

// The use of `Duration` is deprecated and will be removed in a future version
impl Configurable for Duration {
    fn referenceable_name() -> Option<&'static str> {
        Some("stdlib::Duration")
    }

    fn metadata() -> Metadata {
        let mut metadata = Metadata::default();
        metadata.set_description("An duration of time.");
        metadata
    }

    fn generate_schema(_: &RefCell<SchemaGenerator>) -> Result<SchemaObject, GenerateError> {
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

impl ToValue for Duration {
    fn to_value(&self) -> Value {
        serde_json::to_value(self).expect("Could not convert duration to JSON")
    }
}
