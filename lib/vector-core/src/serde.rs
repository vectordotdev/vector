use serde::{de, Deserialize, Deserializer};
use std::{fmt, marker::PhantomData};

/// Answers "Is it possible to skip serializing this value, because it's the
/// default?"
#[inline]
pub fn skip_serializing_if_default<E: Default + PartialEq>(e: &E) -> bool {
    e == &E::default()
}

/// Enables deserializing from a value that could be a bool or a struct.
///
/// Example:
/// healthcheck: bool
/// healthcheck.enabled: bool
/// Both are accepted.
///
/// # Errors
///
/// Returns the error from deserializing the underlying struct.
///
pub fn bool_or_struct<'de, T, D>(deserializer: D) -> Result<T, D::Error>
where
    T: Deserialize<'de> + From<bool>,
    D: Deserializer<'de>,
{
    struct BoolOrStruct<T>(PhantomData<fn() -> T>);

    impl<'de, T> de::Visitor<'de> for BoolOrStruct<T>
    where
        T: Deserialize<'de> + From<bool>,
    {
        type Value = T;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("bool or map")
        }

        fn visit_bool<E>(self, value: bool) -> Result<T, E>
        where
            E: de::Error,
        {
            Ok(value.into())
        }

        fn visit_map<M>(self, map: M) -> Result<T, M::Error>
        where
            M: de::MapAccess<'de>,
        {
            Deserialize::deserialize(de::value::MapAccessDeserializer::new(map))
        }
    }

    deserializer.deserialize_any(BoolOrStruct(PhantomData))
}
