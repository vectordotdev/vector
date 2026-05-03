use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use bytes::{Buf, Bytes};
use chrono::Utc;
use lookup::event_path;
use serde::{Deserialize, Serialize};
use smallvec::{SmallVec, smallvec};
use vector_config::configurable_component;
use vector_core::{
    config::{DataType, LogNamespace, log_schema},
    event::{Event, LogEvent},
    schema,
};
use vrl::value::KeyString;

use super::Deserializer;
use crate::encoding::AvroSerializerOptions;

type VrlValue = vrl::value::Value;
type AvroValue = apache_avro::types::Value;

const CONFLUENT_MAGIC_BYTE: u8 = 0;
const CONFLUENT_SCHEMA_PREFIX_LEN: usize = 5;
const SCHEMA_REGISTRY_CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const SCHEMA_REGISTRY_REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

/// Config used to build a `AvroDeserializer`.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AvroDeserializerConfig {
    /// Options for the Avro deserializer.
    pub avro_options: AvroDeserializerOptions,
}

impl AvroDeserializerConfig {
    /// Creates a new `AvroDeserializerConfig`.
    pub const fn new(schema: String, strip_schema_id_prefix: bool) -> Self {
        Self {
            avro_options: AvroDeserializerOptions {
                schema: Some(schema),
                strip_schema_id_prefix,
                schema_registry: None,
            },
        }
    }

    /// Build the `AvroDeserializer` from this configuration.
    pub fn build(&self) -> vector_common::Result<AvroDeserializer> {
        let opts = &self.avro_options;

        if opts.schema_registry.is_some() && opts.schema.is_some() {
            return Err(
                "avro decoder: `schema` and `schema_registry` are mutually exclusive".into(),
            );
        }

        if let Some(registry) = &opts.schema_registry {
            let http_client = reqwest::Client::builder()
                .connect_timeout(SCHEMA_REGISTRY_CONNECT_TIMEOUT)
                .timeout(SCHEMA_REGISTRY_REQUEST_TIMEOUT)
                .build()
                .map_err(|e| format!("Failed to build schema registry HTTP client: {e}"))?;

            return Ok(AvroDeserializer {
                source: SchemaSource::DynamicRegistry {
                    registry: registry.clone(),
                    http_client,
                    cache: Arc::new(RwLock::new(HashMap::new())),
                },
            });
        }

        let schema_str = opts
            .schema
            .as_deref()
            .ok_or("avro decoder: `schema` is required when `schema_registry` is not set")?;
        let schema = apache_avro::Schema::parse_str(schema_str)
            .map_err(|error| format!("Failed building Avro deserializer: {error}"))?;
        Ok(AvroDeserializer {
            source: SchemaSource::Inline {
                schema,
                strip_schema_id_prefix: opts.strip_schema_id_prefix,
            },
        })
    }

    /// The data type of events that are accepted by `AvroDeserializer`.
    pub fn output_type(&self) -> DataType {
        DataType::Log
    }

    /// The schema required by the serializer.
    pub fn schema_definition(&self, log_namespace: LogNamespace) -> schema::Definition {
        match log_namespace {
            LogNamespace::Legacy => {
                let mut definition = schema::Definition::empty_legacy_namespace()
                    .unknown_fields(vrl::value::Kind::any());

                if let Some(timestamp_key) = log_schema().timestamp_key() {
                    definition = definition.try_with_field(
                        timestamp_key,
                        vrl::value::Kind::any().or_timestamp(),
                        Some("timestamp"),
                    );
                }
                definition
            }
            LogNamespace::Vector => schema::Definition::new_with_default_metadata(
                vrl::value::Kind::any(),
                [log_namespace],
            ),
        }
    }
}

impl From<&AvroDeserializerOptions> for AvroSerializerOptions {
    fn from(value: &AvroDeserializerOptions) -> Self {
        Self {
            schema: value.schema.clone().unwrap_or_default(),
        }
    }
}

/// Apache Avro deserializer options.
#[configurable_component]
#[derive(Clone, Debug)]
pub struct AvroDeserializerOptions {
    /// The Avro schema definition.
    ///
    /// Required when `schema_registry` is not set.
    ///
    /// **Note**: The following [`apache_avro::types::Value`] variants are *not* supported:
    /// * `Date`
    /// * `Decimal`
    /// * `Duration`
    /// * `Fixed`
    /// * `TimeMillis`
    #[configurable(metadata(
        docs::examples = r#"{ "type": "record", "name": "log", "fields": [{ "name": "message", "type": "string" }] }"#,
        docs::additional_props_description = r#"Supports most avro data types, unsupported data types includes
        ["decimal", "duration", "local-timestamp-millis", "local-timestamp-micros"]"#,
    ))]
    #[serde(default)]
    pub schema: Option<String>,

    /// For Avro datum encoded in Kafka messages, the bytes are prefixed with the schema ID.
    /// Set this to `true` to strip the schema ID prefix.
    ///
    /// According to [Confluent Kafka's document](https://docs.confluent.io/platform/current/schema-registry/fundamentals/serdes-develop/index.html#wire-format).
    ///
    /// This option is ignored when `schema_registry` is set — in that case the prefix is
    /// always consumed to extract the schema ID.
    #[serde(default)]
    pub strip_schema_id_prefix: bool,

    /// Schema registry configuration for fetching Avro schemas dynamically.
    ///
    /// When set, each message must use the [Confluent wire format][wire_format] (a 5-byte prefix
    /// containing a magic byte and a 4-byte big-endian schema ID). The schema is fetched from the
    /// registry on first use and cached locally for subsequent messages.
    ///
    /// Mutually exclusive with the inline `schema` field.
    ///
    /// [wire_format]: https://docs.confluent.io/platform/current/schema-registry/fundamentals/serdes-develop/index.html#wire-format
    #[serde(default)]
    pub schema_registry: Option<SchemaRegistryConfig>,
}

/// [Confluent Schema Registry](https://docs.confluent.io/platform/current/schema-registry/index.html) configuration.
#[configurable_component]
#[derive(Clone, Debug)]
pub struct SchemaRegistryConfig {
    /// Base URL of the Confluent Schema Registry, e.g. `http://schema-registry:8081`.
    pub url: String,
}

impl SchemaRegistryConfig {
    fn fetch_schema(
        &self,
        http_client: &reqwest::Client,
        schema_id: u32,
    ) -> vector_common::Result<String> {
        #[derive(Deserialize)]
        struct RegistryResponse {
            schema: String,
        }

        let fetch_url = format!(
            "{}/schemas/ids/{}",
            self.url.trim_end_matches('/'),
            schema_id
        );

        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                let resp = http_client
                    .get(&fetch_url)
                    .send()
                    .await
                    .map_err(|e| {
                        format!("Schema registry request failed (schema_id={schema_id}): {e}")
                    })?
                    .error_for_status()
                    .map_err(|e| {
                        format!("Schema registry returned error (schema_id={schema_id}): {e}")
                    })?;

                let body: RegistryResponse = resp.json().await.map_err(|e| {
                    format!("Failed to parse schema registry response (schema_id={schema_id}): {e}")
                })?;

                Ok(body.schema)
            })
        })
    }
}

/// Internal schema source for `AvroDeserializer`.
#[derive(Debug, Clone)]
enum SchemaSource {
    /// Schema is provided inline in the config.
    Inline {
        schema: apache_avro::Schema,
        strip_schema_id_prefix: bool,
    },
    /// Schema is fetched dynamically from a schema registry using the ID
    /// embedded in each message's Confluent wire-format prefix.
    DynamicRegistry {
        registry: SchemaRegistryConfig,
        http_client: reqwest::Client,
        cache: Arc<RwLock<HashMap<u32, apache_avro::Schema>>>,
    },
}

/// Deserializer that converts bytes to an `Event` using the Apache Avro format.
#[derive(Debug, Clone)]
pub struct AvroDeserializer {
    source: SchemaSource,
}

impl AvroDeserializer {
    /// Creates a new `AvroDeserializer` with an inline schema.
    pub fn new(schema: apache_avro::Schema, strip_schema_id_prefix: bool) -> Self {
        Self {
            source: SchemaSource::Inline {
                schema,
                strip_schema_id_prefix,
            },
        }
    }
}

fn decode_avro(
    schema: &apache_avro::Schema,
    bytes: Bytes,
    log_namespace: LogNamespace,
) -> vector_common::Result<SmallVec<[Event; 1]>> {
    let value = apache_avro::from_avro_datum(schema, &mut bytes.reader(), None)?;

    let apache_avro::types::Value::Record(fields) = value else {
        return Err(vector_common::Error::from("Expected an avro Record"));
    };

    let mut log = LogEvent::default();
    for (k, v) in fields {
        log.insert(event_path!(k.as_str()), try_from(v)?);
    }

    let mut event = Event::Log(log);
    let event = match log_namespace {
        LogNamespace::Vector => event,
        LogNamespace::Legacy => {
            if let Some(timestamp_key) = log_schema().timestamp_key_target_path() {
                let log = event.as_mut_log();
                if !log.contains(timestamp_key) {
                    log.insert(timestamp_key, Utc::now());
                }
            }
            event
        }
    };
    Ok(smallvec![event])
}

impl Deserializer for AvroDeserializer {
    fn parse(
        &self,
        bytes: Bytes,
        log_namespace: LogNamespace,
    ) -> vector_common::Result<SmallVec<[Event; 1]>> {
        // Avro has a `null` type which indicates no value.
        if bytes.is_empty() {
            return Ok(smallvec![]);
        }

        match &self.source {
            SchemaSource::Inline {
                schema,
                strip_schema_id_prefix,
            } => {
                let bytes = if *strip_schema_id_prefix {
                    if bytes.len() >= CONFLUENT_SCHEMA_PREFIX_LEN
                        && bytes[0] == CONFLUENT_MAGIC_BYTE
                    {
                        bytes.slice(CONFLUENT_SCHEMA_PREFIX_LEN..)
                    } else {
                        return Err(vector_common::Error::from(
                            "Expected avro datum to be prefixed with schema id",
                        ));
                    }
                } else {
                    bytes
                };
                decode_avro(schema, bytes, log_namespace)
            }

            SchemaSource::DynamicRegistry {
                registry,
                http_client,
                cache,
            } => {
                if bytes.len() < CONFLUENT_SCHEMA_PREFIX_LEN || bytes[0] != CONFLUENT_MAGIC_BYTE {
                    return Err(vector_common::Error::from(
                        "Expected Confluent wire-format prefix (magic byte + 4-byte schema ID)",
                    ));
                }
                let schema_id = u32::from_be_bytes([bytes[1], bytes[2], bytes[3], bytes[4]]);
                let payload = bytes.slice(CONFLUENT_SCHEMA_PREFIX_LEN..);

                {
                    let guard = cache
                        .read()
                        .map_err(|e| format!("Schema cache lock poisoned: {e}"))?;
                    if let Some(schema) = guard.get(&schema_id) {
                        return decode_avro(schema, payload, log_namespace);
                    }
                }

                let schema_str = registry.fetch_schema(http_client, schema_id)?;
                let schema = apache_avro::Schema::parse_str(&schema_str).map_err(|e| {
                    format!("Failed to parse schema from registry (schema_id={schema_id}): {e}")
                })?;

                let mut guard = cache
                    .write()
                    .map_err(|e| format!("Schema cache lock poisoned: {e}"))?;
                // Another thread may have populated the cache while we were fetching.
                let schema = guard.entry(schema_id).or_insert(schema);
                decode_avro(schema, payload, log_namespace)
            }
        }
    }
}

// Can't use std::convert::TryFrom because of orphan rules
pub fn try_from(value: AvroValue) -> vector_common::Result<VrlValue> {
    // Very similar to avro to json see `impl std::convert::TryFrom<AvroValue> for serde_json::Value`
    // LogEvent has native support for bytes, so it is used for Bytes and Fixed
    match value {
        AvroValue::Array(array) => {
            let mut vector = Vec::new();
            for item in array {
                vector.push(try_from(item)?);
            }
            Ok(VrlValue::Array(vector))
        }
        AvroValue::Boolean(boolean) => Ok(VrlValue::from(boolean)),
        AvroValue::Bytes(bytes) => Ok(VrlValue::from(bytes)),
        AvroValue::Date(_) => Err(vector_common::Error::from(
            "AvroValue::Date is not supported",
        )),
        AvroValue::Decimal(_) => Err(vector_common::Error::from(
            "AvroValue::Decimal is not supported",
        )),
        AvroValue::Double(double) => Ok(VrlValue::from_f64_or_zero(double)),
        AvroValue::Duration(_) => Err(vector_common::Error::from(
            "AvroValue::Duration is not supported",
        )),
        AvroValue::Enum(_, string) => Ok(VrlValue::from(string)),
        AvroValue::Fixed(_, _) => Err(vector_common::Error::from(
            "AvroValue::Fixed is not supported",
        )),
        AvroValue::Float(float) => Ok(VrlValue::from_f64_or_zero(float as f64)),
        AvroValue::Int(int) => Ok(VrlValue::from(int)),
        AvroValue::Long(long) => Ok(VrlValue::from(long)),
        AvroValue::Map(items) => items
            .into_iter()
            .map(|(key, value)| try_from(value).map(|v| (KeyString::from(key), v)))
            .collect::<Result<Vec<_>, _>>()
            .map(|v| VrlValue::Object(v.into_iter().collect())),
        AvroValue::Null => Ok(VrlValue::Null),
        AvroValue::Record(items) => items
            .into_iter()
            .map(|(key, value)| try_from(value).map(|v| (KeyString::from(key), v)))
            .collect::<Result<Vec<_>, _>>()
            .map(|v| VrlValue::Object(v.into_iter().collect())),
        AvroValue::String(string) => Ok(VrlValue::from(string)),
        AvroValue::TimeMicros(time_micros) => Ok(VrlValue::from(time_micros)),
        AvroValue::TimeMillis(_) => Err(vector_common::Error::from(
            "AvroValue::TimeMillis is not supported",
        )),
        AvroValue::TimestampMicros(ts_micros) => Ok(VrlValue::from(ts_micros)),
        AvroValue::TimestampMillis(ts_millis) => Ok(VrlValue::from(ts_millis)),
        AvroValue::Union(_, v) => try_from(*v),
        AvroValue::Uuid(uuid) => Ok(VrlValue::from(uuid.as_hyphenated().to_string())),
        AvroValue::LocalTimestampMillis(ts_millis) => Ok(VrlValue::from(ts_millis)),
        AvroValue::LocalTimestampMicros(ts_micros) => Ok(VrlValue::from(ts_micros)),
        AvroValue::BigDecimal(_) => Err(vector_common::Error::from(
            "AvroValue::BigDecimal is not supported",
        )),
        AvroValue::TimestampNanos(_) => Err(vector_common::Error::from(
            "AvroValue::TimestampNanos is not supported",
        )),
        AvroValue::LocalTimestampNanos(_) => Err(vector_common::Error::from(
            "AvroValue::LocalTimestampNanos is not supported",
        )),
    }
}

#[cfg(test)]
mod tests {
    use apache_avro::Schema;
    use bytes::BytesMut;
    use uuid::Uuid;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;

    #[derive(Debug, Clone, Serialize, Deserialize)]
    struct Log {
        message: String,
    }

    fn get_schema() -> Schema {
        let schema = String::from(
            r#"{
                "type": "record",
                "name": "log",
                "fields": [
                    {
                        "name": "message",
                        "type": "string"
                    }
                ]
            }
        "#,
        );

        Schema::parse_str(&schema).unwrap()
    }

    #[test]
    fn deserialize_avro() {
        let schema = get_schema();

        let event = Log {
            message: "hello from avro".to_owned(),
        };
        let record_value = apache_avro::to_value(event).unwrap();
        let record_datum = apache_avro::to_avro_datum(&schema, record_value).unwrap();
        let record_bytes = Bytes::from(record_datum);

        let deserializer = AvroDeserializer::new(schema, false);
        let events = deserializer
            .parse(record_bytes, LogNamespace::Vector)
            .unwrap();
        assert_eq!(events.len(), 1);

        assert_eq!(
            events[0].as_log().get("message").unwrap(),
            &VrlValue::from("hello from avro")
        );
    }

    #[test]
    fn deserialize_avro_strip_schema_id_prefix() {
        let schema = get_schema();

        let event = Log {
            message: "hello from avro".to_owned(),
        };
        let record_value = apache_avro::to_value(event).unwrap();
        let record_datum = apache_avro::to_avro_datum(&schema, record_value).unwrap();

        let mut bytes = BytesMut::new();
        bytes.extend([0, 0, 0, 0, 0]); // 0 prefix + 4 byte schema id
        bytes.extend(record_datum);

        let deserializer = AvroDeserializer::new(schema, true);
        let events = deserializer
            .parse(bytes.freeze(), LogNamespace::Vector)
            .unwrap();
        assert_eq!(events.len(), 1);

        assert_eq!(
            events[0].as_log().get("message").unwrap(),
            &VrlValue::from("hello from avro")
        );
    }

    #[test]
    fn deserialize_avro_uuid() {
        let schema = get_schema();

        let uuid = Uuid::new_v4().hyphenated().to_string();
        let event = Log {
            message: uuid.clone(),
        };
        let value = apache_avro::to_value(event).unwrap();
        // let value = value.resolve(&schema).unwrap();
        let datum = apache_avro::to_avro_datum(&schema, value).unwrap();

        let mut bytes = BytesMut::new();
        bytes.extend([0, 0, 0, 0, 0]); // 0 prefix + 4 byte schema id
        bytes.extend(datum);

        let deserializer = AvroDeserializer::new(schema, true);
        let events = deserializer
            .parse(bytes.freeze(), LogNamespace::Vector)
            .unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(
            events[0].as_log().get("message").unwrap(),
            &VrlValue::from(uuid)
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn deserialize_avro_schema_registry() {
        let schema = get_schema();
        let schema_json =
            r#"{"type":"record","name":"log","fields":[{"name":"message","type":"string"}]}"#;
        let schema_id: u32 = 42;

        // Start a mock schema registry server.
        let mock_server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path(format!("/schemas/ids/{schema_id}")))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({ "schema": schema_json })),
            )
            .expect(1) // Should only be fetched once during the first message.
            .mount(&mock_server)
            .await;

        let deserializer = AvroDeserializerConfig {
            avro_options: AvroDeserializerOptions {
                schema: None,
                strip_schema_id_prefix: false,
                schema_registry: Some(SchemaRegistryConfig {
                    url: mock_server.uri(),
                }),
            },
        }
        .build()
        .unwrap();

        // Build a message with Confluent wire-format prefix.
        let make_message = |schema: &Schema, msg: &str| {
            let event = Log {
                message: msg.to_owned(),
            };
            let value = apache_avro::to_value(event).unwrap();
            let datum = apache_avro::to_avro_datum(schema, value).unwrap();
            let mut bytes = BytesMut::new();
            bytes.extend([CONFLUENT_MAGIC_BYTE]);
            bytes.extend(schema_id.to_be_bytes());
            bytes.extend(datum);
            bytes.freeze()
        };

        // First message should fetch schema from the registry dynamically.
        let events = deserializer
            .parse(make_message(&schema, "first"), LogNamespace::Vector)
            .unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(
            events[0].as_log().get("message").unwrap(),
            &VrlValue::from("first")
        );

        // Second message with same schema ID should use the cache.
        let events = deserializer
            .parse(make_message(&schema, "second"), LogNamespace::Vector)
            .unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(
            events[0].as_log().get("message").unwrap(),
            &VrlValue::from("second")
        );
    }

    #[test]
    fn deserialize_avro_registry_rejects_missing_prefix() {
        let deserializer = AvroDeserializerConfig {
            avro_options: AvroDeserializerOptions {
                schema: None,
                strip_schema_id_prefix: false,
                schema_registry: Some(SchemaRegistryConfig {
                    url: "http://localhost:8081".to_string(),
                }),
            },
        }
        .build()
        .unwrap();

        // Raw Avro bytes without the Confluent prefix should be rejected.
        let schema = get_schema();
        let event = Log {
            message: "no prefix".to_owned(),
        };
        let value = apache_avro::to_value(event).unwrap();
        let datum = apache_avro::to_avro_datum(&schema, value).unwrap();

        let result = deserializer.parse(Bytes::from(datum), LogNamespace::Vector);
        assert!(result.is_err());
    }
}
