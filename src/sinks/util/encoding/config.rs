use crate::{
    event::{PathComponent, PathIter},
    sinks::util::encoding::{
        with_default::EncodingConfigWithDefault, EncodingConfiguration, TimestampFormat,
    },
};
use serde::{
    de::{self, DeserializeOwned, IntoDeserializer, MapAccess, Visitor},
    Deserialize, Deserializer, Serialize,
};
use std::{
    fmt::{self, Debug},
    marker::PhantomData,
};
use string_cache::DefaultAtom as Atom;

/// A structure to wrap sink encodings and enforce field privacy.
///
/// This structure **does not** assume that there is a default format. Consider
/// `EncodingConfigWithDefault<E>` instead if `E: Default`.
#[derive(Serialize, Debug, Eq, PartialEq, Clone)]
#[serde(deny_unknown_fields)]
pub struct EncodingConfig<E> {
    pub(crate) codec: E,
    // TODO(2410): Using PathComponents here is a hack for #2407, #2410 should fix this fully.
    #[serde(default)]
    pub(crate) only_fields: Option<Vec<Vec<PathComponent>>>,
    #[serde(default)]
    pub(crate) except_fields: Option<Vec<Atom>>,
    #[serde(default)]
    pub(crate) timestamp_format: Option<TimestampFormat>,
}

impl<E> EncodingConfiguration<E> for EncodingConfig<E> {
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

impl<E> Into<EncodingConfigWithDefault<E>> for EncodingConfig<E>
where
    E: Default + PartialEq,
{
    fn into(self) -> EncodingConfigWithDefault<E> {
        EncodingConfigWithDefault {
            codec: self.codec,
            only_fields: self.only_fields,
            except_fields: self.except_fields,
            timestamp_format: self.timestamp_format,
        }
    }
}

impl<E> From<E> for EncodingConfig<E> {
    fn from(codec: E) -> Self {
        Self {
            codec,
            only_fields: Default::default(),
            except_fields: Default::default(),
            timestamp_format: Default::default(),
        }
    }
}

impl<'de, E> Deserialize<'de> for EncodingConfig<E>
where
    E: DeserializeOwned + Serialize + Debug + Clone + PartialEq + Eq,
{
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        // This is a Visitor that forwards string types to T's `FromStr` impl and
        // forwards map types to T's `Deserialize` impl. The `PhantomData` is to
        // keep the compiler from complaining about T being an unused generic type
        // parameter. We need T in order to know the Value type for the Visitor
        // impl.
        struct StringOrStruct<T: DeserializeOwned + Serialize + Debug + Eq + PartialEq + Clone>(
            PhantomData<fn() -> T>,
        );

        impl<'de, T> Visitor<'de> for StringOrStruct<T>
        where
            T: DeserializeOwned + Serialize + Debug + Eq + PartialEq + Clone,
        {
            type Value = Inner<T>;

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

#[derive(Deserialize, Serialize, Debug, Eq, PartialEq, Clone)]
pub struct Inner<E> {
    codec: E,
    #[serde(default)]
    only_fields: Option<Vec<String>>,
    #[serde(default)]
    except_fields: Option<Vec<Atom>>,
    #[serde(default)]
    timestamp_format: Option<TimestampFormat>,
}
