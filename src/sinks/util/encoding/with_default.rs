use std::{
    fmt::{self, Debug},
    marker::PhantomData,
};

use serde::{
    de::{self, DeserializeOwned, Deserializer, IntoDeserializer, MapAccess, Visitor},
    Deserialize, Serialize,
};

use crate::{
    event::{PathComponent, PathIter},
    serde::skip_serializing_if_default,
    sinks::util::encoding::{EncodingConfiguration, TimestampFormat},
};

/// A structure to wrap sink encodings and enforce field privacy.
///
/// This structure **does** assume that there is a default format. Consider
/// `EncodingConfig<E>` instead if `E: !Default`.
#[derive(Serialize, Debug, Eq, PartialEq, Clone, Default)]
pub struct EncodingConfigWithDefault<E: Default + PartialEq> {
    /// The format of the encoding.
    // TODO: This is currently sink specific.
    #[serde(default, skip_serializing_if = "skip_serializing_if_default")]
    pub(crate) codec: E,
    #[serde(default, skip_serializing_if = "skip_serializing_if_default")]
    pub(crate) schema: Option<String>,
    /// Keep only the following fields of the message. (Items mutually exclusive with `except_fields`)
    #[serde(default, skip_serializing_if = "skip_serializing_if_default")]
    // TODO(2410): Using PathComponents here is a hack for #2407, #2410 should fix this fully.
    pub(crate) only_fields: Option<Vec<Vec<PathComponent<'static>>>>,
    /// Remove the following fields of the message. (Items mutually exclusive with `only_fields`)
    #[serde(default, skip_serializing_if = "skip_serializing_if_default")]
    pub(crate) except_fields: Option<Vec<String>>,
    /// Format for outgoing timestamps.
    #[serde(default, skip_serializing_if = "skip_serializing_if_default")]
    pub(crate) timestamp_format: Option<TimestampFormat>,
}

impl<E: Default + PartialEq> EncodingConfiguration for EncodingConfigWithDefault<E> {
    type Codec = E;

    fn codec(&self) -> &Self::Codec {
        &self.codec
    }

    fn schema(&self) -> &Option<String> {
        &self.schema
    }

    // TODO(2410): Using PathComponents here is a hack for #2407, #2410 should fix this fully.
    fn only_fields(&self) -> &Option<Vec<Vec<PathComponent<'static>>>> {
        &self.only_fields
    }

    fn except_fields(&self) -> &Option<Vec<String>> {
        &self.except_fields
    }

    fn timestamp_format(&self) -> &Option<TimestampFormat> {
        &self.timestamp_format
    }
}

impl<E> From<E> for EncodingConfigWithDefault<E>
where
    E: Default + PartialEq,
{
    fn from(codec: E) -> Self {
        Self {
            codec,
            schema: Default::default(),
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
                    schema: Default::default(),
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
            schema: inner.schema,
            // TODO(2410): Using PathComponents here is a hack for #2407, #2410 should fix this fully.
            only_fields: inner.only_fields.map(|fields| {
                fields
                    .iter()
                    .map(|only| {
                        PathIter::new(only)
                            .map(|component| component.into_static())
                            .collect()
                    })
                    .collect()
            }),
            except_fields: inner.except_fields,
            timestamp_format: inner.timestamp_format,
        };

        concrete.validate().map_err(de::Error::custom)?;
        Ok(concrete)
    }
}

#[derive(Deserialize, Serialize, Debug, Default, Eq, PartialEq, Clone)]
pub struct InnerWithDefault<E: Default> {
    #[serde(default)]
    codec: E,
    #[serde(default)]
    schema: Option<String>,
    #[serde(default)]
    only_fields: Option<Vec<String>>,
    #[serde(default)]
    except_fields: Option<Vec<String>>,
    #[serde(default)]
    timestamp_format: Option<TimestampFormat>,
}
