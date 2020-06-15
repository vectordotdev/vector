use crate::{
    event::{PathComponent, PathIter},
    sinks::util::encoding::{EncodingConfig, EncodingConfiguration, TimestampFormat},
};
use serde::{
    de::{self, DeserializeOwned, Deserializer, IntoDeserializer, MapAccess, Visitor},
    Deserialize, Serialize,
};
use std::{
    fmt::{self, Debug},
    marker::PhantomData,
};
use string_cache::DefaultAtom as Atom;

/// A structure to wrap sink encodings and enforce field privacy.
///
/// This structure **does** assume that there is a default format. Consider
/// `EncodingConfig<E>` instead if `E: !Default`.
#[derive(Serialize, Debug, Eq, PartialEq, Clone, Default)]
pub struct EncodingConfigWithDefault<E: Default + PartialEq> {
    /// The format of the encoding.
    // TODO: This is currently sink specific.
    #[serde(
        default,
        skip_serializing_if = "crate::serde::skip_serializing_if_default"
    )]
    pub(crate) codec: E,
    /// Keep only the following fields of the message. (Items mutually exclusive with `except_fields`)
    #[serde(
        default,
        skip_serializing_if = "crate::serde::skip_serializing_if_default"
    )]
    // TODO(2410): Using PathComponents here is a hack for #2407, #2410 should fix this fully.
    pub(crate) only_fields: Option<Vec<Vec<PathComponent>>>,
    /// Remove the following fields of the message. (Items mutually exclusive with `only_fields`)
    #[serde(
        default,
        skip_serializing_if = "crate::serde::skip_serializing_if_default"
    )]
    pub(crate) except_fields: Option<Vec<Atom>>,
    /// Format for outgoing timestamps.
    #[serde(
        default,
        skip_serializing_if = "crate::serde::skip_serializing_if_default"
    )]
    pub(crate) timestamp_format: Option<TimestampFormat>,
}

impl<E: Default + PartialEq> EncodingConfiguration<E> for EncodingConfigWithDefault<E> {
    fn codec(&self) -> &E {
        &self.codec
    }
    // TODO(2410): Using PathComponents here is a hack for #2407, #2410 should fix this fully.
    fn only_fields(&self) -> &Option<Vec<Vec<PathComponent>>> {
        &self.only_fields
    }
    fn except_fields(&self) -> &Option<Vec<Atom>> {
        &self.except_fields
    }
    fn timestamp_format(&self) -> &Option<TimestampFormat> {
        &self.timestamp_format
    }
}

impl<E> EncodingConfigWithDefault<E>
where
    E: Default + PartialEq,
{
    #[allow(dead_code)] // Required for `make check-component-features`
    pub(crate) fn transmute<X>(self) -> EncodingConfigWithDefault<X>
    where
        X: From<E> + Default + PartialEq,
    {
        EncodingConfigWithDefault {
            codec: self.codec.into(),
            only_fields: self.only_fields,
            except_fields: self.except_fields,
            timestamp_format: self.timestamp_format,
        }
    }
    #[allow(dead_code)] // Required for `make check-component-features`
    pub(crate) fn without_default<X>(self) -> EncodingConfig<X>
    where
        X: From<E> + PartialEq,
    {
        EncodingConfig {
            codec: self.codec.into(),
            only_fields: self.only_fields,
            except_fields: self.except_fields,
            timestamp_format: self.timestamp_format,
        }
    }
}

impl<E> Into<EncodingConfig<E>> for EncodingConfigWithDefault<E>
where
    E: Default + PartialEq,
{
    fn into(self) -> EncodingConfig<E> {
        let Self {
            codec,
            only_fields,
            except_fields,
            timestamp_format,
        } = self;
        EncodingConfig {
            codec,
            only_fields,
            except_fields,
            timestamp_format,
        }
    }
}

impl<E: Default + PartialEq> From<E> for EncodingConfigWithDefault<E> {
    fn from(codec: E) -> Self {
        Self {
            codec,
            only_fields: Default::default(),
            except_fields: Default::default(),
            timestamp_format: Default::default(),
        }
    }
}

impl<'de, E> Deserialize<'de> for EncodingConfigWithDefault<E>
where
    E: DeserializeOwned + Serialize + Debug + Clone + PartialEq + Eq + Default,
{
    // Derived from https://serde.rs/string-or-struct.html
    #[allow(dead_code)] // For supporting `--no-default-features`
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        // This is a Visitor that forwards string types to T's `FromStr` impl and
        // forwards map types to T's `Deserialize` impl. The `PhantomData` is to
        // keep the compiler from complaining about T being an unused generic type
        // parameter. We need T in order to know the Value type for the Visitor
        // impl.
        struct StringOrStruct<T: DeserializeOwned + Serialize + Debug + Eq + PartialEq + Clone + Default>(
            PhantomData<fn() -> T>,
        );

        impl<'de, T> Visitor<'de> for StringOrStruct<T>
        where
            T: DeserializeOwned + Serialize + Debug + Eq + PartialEq + Clone + Default,
        {
            type Value = InnerWithDefault<T>;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("string or map")
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(Self::Value {
                    codec: T::deserialize(value.into_deserializer())?,
                    only_fields: Default::default(),
                    except_fields: Default::default(),
                    timestamp_format: Default::default(),
                })
            }

            fn visit_map<M>(self, map: M) -> std::result::Result<Self::Value, M::Error>
            where
                M: MapAccess<'de>,
            {
                // `MapAccessDeserializer` is a wrapper that turns a `MapAccess`
                // into a `Deserializer`, allowing it to be used as the input to T's
                // `Deserialize` implementation. T then deserializes itself using
                // the entries from the map visitor.
                Deserialize::deserialize(de::value::MapAccessDeserializer::new(map))
            }
        }

        let inner = deserializer.deserialize_any(StringOrStruct::<E>(PhantomData))?;

        let concrete = Self {
            codec: inner.codec,
            // TODO(2410): Using PathComponents here is a hack for #2407, #2410 should fix this fully.
            only_fields: inner.only_fields.map(|fields| {
                fields
                    .iter()
                    .map(|only| PathIter::new(only).collect())
                    .collect()
            }),
            except_fields: inner.except_fields,
            timestamp_format: inner.timestamp_format,
        };

        concrete
            .validate()
            .map_err(|e| serde::de::Error::custom(e))?;
        Ok(concrete)
    }
}

#[derive(Deserialize, Serialize, Debug, Default, Eq, PartialEq, Clone)]
pub struct InnerWithDefault<E: Default> {
    #[serde(default)]
    codec: E,
    #[serde(default)]
    only_fields: Option<Vec<String>>,
    #[serde(default)]
    except_fields: Option<Vec<Atom>>,
    #[serde(default)]
    timestamp_format: Option<TimestampFormat>,
}
