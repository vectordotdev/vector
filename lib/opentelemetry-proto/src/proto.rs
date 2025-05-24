/// Service stub and clients.
pub mod collector {
    pub mod trace {
        pub mod v1 {
            tonic::include_proto!("opentelemetry.proto.collector.trace.v1");
        }
    }
    pub mod logs {
        pub mod v1 {
            tonic::include_proto!("opentelemetry.proto.collector.logs.v1");
        }
    }
    pub mod metrics {
        pub mod v1 {
            tonic::include_proto!("opentelemetry.proto.collector.metrics.v1");
        }
    }
}

/// Common types used across all event types.
pub mod common {
    pub mod v1 {
        tonic::include_proto!("opentelemetry.proto.common.v1");
    }
}

/// Generated types used for logs.
pub mod logs {
    pub mod v1 {
        tonic::include_proto!("opentelemetry.proto.logs.v1");
    }
}

/// Generated types used for metrics.
pub mod metrics {
    pub mod v1 {
        tonic::include_proto!("opentelemetry.proto.metrics.v1");
    }
}

/// Generated types used for trace.
pub mod trace {
    pub mod v1 {
        tonic::include_proto!("opentelemetry.proto.trace.v1");
    }
}

/// Generated types used in resources.
pub mod resource {
    pub mod v1 {
        tonic::include_proto!("opentelemetry.proto.resource.v1");
    }
}

#[cfg(all(feature = "with-serde"))]
pub(crate) mod serializers {
    use serde::de::{self, MapAccess, Visitor};
    use serde::ser::{SerializeMap, SerializeStruct};
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use serde_json::Value;
    use std::fmt;

    // hex string <-> bytes conversion

    pub fn serialize_to_hex_string<S>(bytes: &[u8], serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let hex_string = hex::encode(bytes);
        serializer.serialize_str(&hex_string)
    }

    pub fn deserialize_from_hex_string<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct BytesVisitor;

        impl<'de> Visitor<'de> for BytesVisitor {
            type Value = Vec<u8>;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a string representing hex-encoded bytes")
            }

            fn visit_str<E>(self, value: &str) -> Result<Vec<u8>, E>
            where
                E: de::Error,
            {
                hex::decode(value).map_err(E::custom)
            }
        }

        deserializer.deserialize_str(BytesVisitor)
    }

    pub fn deserialize_from_str_or_u64_to_u64<'de, D>(deserializer: D) -> Result<u64, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct IntOrStr;

        impl<'de> Visitor<'de> for IntOrStr {
            type Value = u64;

            fn expecting(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
                fmt.write_str("u64 or string encoded u64")
            }

            fn visit_u64<E>(self, val: u64) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                Ok(val)
            }

            fn visit_str<E>(self, val: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                val.parse::<u64>().map_err(de::Error::custom)
            }
        }

        deserializer.deserialize_any(IntOrStr)
    }

    pub fn serialize_u64_to_string<S>(value: &u64, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let s = value.to_string();
        serializer.serialize_str(&s)
    }

    pub fn deserialize_string_to_u64<'de, D>(deserializer: D) -> Result<u64, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s: String = Deserialize::deserialize(deserializer)?;
        s.parse::<u64>().map_err(de::Error::custom)
    }
}
