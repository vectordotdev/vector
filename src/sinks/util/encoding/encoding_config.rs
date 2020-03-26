use crate::sinks::util::encoding::{
    encoding_config_with_default::EncodingConfigWithDefault, inner::Inner, EncodingConfiguration,
    TimestampFormat,
};
use serde::de::{DeserializeOwned, IntoDeserializer, MapAccess, Visitor};
use serde::{de, Deserialize, Deserializer, Serialize};
use std::fmt;
use std::fmt::Debug;
use std::marker::PhantomData;
use string_cache::DefaultAtom as Atom;

/// A structure to wrap sink encodings and enforce field privacy.
///
/// This structure **does not** assume that there is a default format. Consider
/// `EncodingConfigWithDefault<E>` instead if `E: Default`.
#[derive(Serialize, Debug, Eq, PartialEq, Clone)]
#[serde(deny_unknown_fields)]
pub struct EncodingConfig<E> {
    pub(super) codec: E,
    #[serde(default)]
    pub(super) only_fields: Option<Vec<Atom>>,
    #[serde(default)]
    pub(super) except_fields: Option<Vec<Atom>>,
    #[serde(default)]
    pub(super) timestamp_format: Option<TimestampFormat>,
}

impl<E> EncodingConfiguration<E> for EncodingConfig<E> {
    fn codec(&self) -> &E {
        &self.codec
    }
    fn only_fields(&self) -> &Option<Vec<Atom>> {
        &self.only_fields
    }
    fn set_only_fields(&mut self, fields: Option<Vec<Atom>>) -> Option<Vec<Atom>> {
        std::mem::replace(&mut self.only_fields, fields)
    }
    fn except_fields(&self) -> &Option<Vec<Atom>> {
        &self.except_fields
    }
    fn set_except_fields(&mut self, fields: Option<Vec<Atom>>) -> Option<Vec<Atom>> {
        std::mem::replace(&mut self.only_fields, fields)
    }
    fn timestamp_format(&self) -> &Option<TimestampFormat> {
        &self.timestamp_format
    }
    fn set_timestamp_format(&mut self, format: Option<TimestampFormat>) -> Option<TimestampFormat> {
        std::mem::replace(&mut self.timestamp_format, format)
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
            only_fields: inner.only_fields,
            except_fields: inner.except_fields,
            timestamp_format: inner.timestamp_format,
        };

        concrete
            .validate()
            .map_err(|e| serde::de::Error::custom(e))?;
        Ok(concrete)
    }
}
