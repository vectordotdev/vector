use std::fmt::Debug;

use lookup::lookup_v2::OwnedPath;
use serde::{Deserialize, Serialize};

use crate::{
    serde::skip_serializing_if_default,
    sinks::util::encoding::{EncodingConfiguration, TimestampFormat},
};

/// A structure to wrap sink encodings and enforce field privacy.
///
/// This structure requires a codec that can be instantiated entirely via `Default`.  It does not
/// allow changes to the codec to be pushed back during serialization.  This is only useful for
/// sinks that have an entirely fixed encoding i.e. they always encode to JSON, etc.
#[derive(Deserialize, Serialize, Debug, Eq, PartialEq, Clone, Default)]
pub struct EncodingConfigFixed<E: Default + PartialEq> {
    /// The format of the encoding.
    #[serde(skip)]
    pub(crate) codec: E,
    #[serde(default, skip_serializing_if = "skip_serializing_if_default")]
    pub(crate) schema: Option<String>,
    /// Keep only the following fields of the message. (Items mutually exclusive with `except_fields`)
    #[serde(default, skip_serializing_if = "skip_serializing_if_default")]
    pub(crate) only_fields: Option<Vec<OwnedPath>>,
    /// Remove the following fields of the message. (Items mutually exclusive with `only_fields`)
    #[serde(default, skip_serializing_if = "skip_serializing_if_default")]
    pub(crate) except_fields: Option<Vec<String>>,
    /// Format for outgoing timestamps.
    #[serde(default, skip_serializing_if = "skip_serializing_if_default")]
    pub(crate) timestamp_format: Option<TimestampFormat>,
}

impl<E: Default + PartialEq> EncodingConfiguration for EncodingConfigFixed<E> {
    type Codec = E;

    fn codec(&self) -> &Self::Codec {
        &self.codec
    }

    fn schema(&self) -> &Option<String> {
        &self.schema
    }

    fn only_fields(&self) -> &Option<Vec<OwnedPath>> {
        &self.only_fields
    }

    fn except_fields(&self) -> &Option<Vec<String>> {
        &self.except_fields
    }

    fn timestamp_format(&self) -> &Option<TimestampFormat> {
        &self.timestamp_format
    }
}

impl<E> From<E> for EncodingConfigFixed<E>
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
