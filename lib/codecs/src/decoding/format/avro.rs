use bytes::{Buf, Bytes};
use chrono::Utc;
use lookup::event_path;
use serde::{Deserialize, Serialize};
use smallvec::{SmallVec, smallvec};
use vector_common::decompression::max_decompressed_size_bytes;
use vector_config::configurable_component;
use vector_core::{
    config::{DataType, LogNamespace, log_schema},
    event::{Event, LogEvent},
    schema,
};
use vrl::value::KeyString;

use super::Deserializer;
use crate::avro::{AvroEncoding, AvroSchemaSource};
use crate::encoding::AvroSerializerOptions;

type VrlValue = vrl::value::Value;
type AvroValue = apache_avro::types::Value;

const CONFLUENT_MAGIC_BYTE: u8 = 0;
const CONFLUENT_SCHEMA_PREFIX_LEN: usize = 5;
const DEFAULT_MAX_OCF_RECORDS: usize = 100_000;

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
                schema,
                strip_schema_id_prefix,
                encoding: AvroEncoding::Datum,
                schema_source: Some(AvroSchemaSource::Provided),
                max_ocf_records: DEFAULT_MAX_OCF_RECORDS,
            },
        }
    }

    /// Creates a new `AvroDeserializerConfig` with custom options.
    pub const fn new_with_options(
        schema: String,
        strip_schema_id_prefix: bool,
        encoding: AvroEncoding,
        schema_source: AvroSchemaSource,
    ) -> Self {
        Self {
            avro_options: AvroDeserializerOptions {
                schema,
                strip_schema_id_prefix,
                encoding,
                schema_source: Some(schema_source),
                max_ocf_records: DEFAULT_MAX_OCF_RECORDS,
            },
        }
    }

    /// Build the `AvroDeserializer` from this configuration.
    pub fn build(&self) -> vector_common::Result<AvroDeserializer> {
        // strip_schema_id_prefix is a Confluent Schema Registry concept that applies only to
        // raw Avro datum encoding. OCF files have their own schema embedding mechanism and do
        // not use the Confluent wire format prefix.
        if self.avro_options.strip_schema_id_prefix
            && self.avro_options.encoding == AvroEncoding::ObjectContainerFile
        {
            return Err(vector_common::Error::from(
                "`strip_schema_id_prefix` is not compatible with `object_container_file` encoding. \
                 OCF files embed the schema in the file header; they do not use the Confluent \
                 Schema Registry wire format prefix.",
            ));
        }

        if self.avro_options.encoding == AvroEncoding::ObjectContainerFile
            && self.avro_options.max_ocf_records == 0
        {
            return Err(vector_common::Error::from(
                "`max_ocf_records` must be greater than zero for `object_container_file` encoding.",
            ));
        }

        let schema_source =
            self.avro_options
                .schema_source
                .unwrap_or(match self.avro_options.encoding {
                    AvroEncoding::Datum => AvroSchemaSource::Provided,
                    AvroEncoding::ObjectContainerFile => AvroSchemaSource::Embedded,
                });

        let schema = if self.avro_options.encoding == AvroEncoding::ObjectContainerFile
            && schema_source == AvroSchemaSource::Embedded
        {
            // For OCF with embedded schema, we don't need to pre-parse the schema
            None
        } else {
            Some(
                apache_avro::Schema::parse_str(&self.avro_options.schema)
                    .map_err(|error| format!("Failed building Avro deserializer: {error}"))?,
            )
        };

        Ok(AvroDeserializer {
            schema,
            strip_schema_id_prefix: self.avro_options.strip_schema_id_prefix,
            encoding: self.avro_options.encoding,
            schema_source,
            max_ocf_bytes: max_decompressed_size_bytes(),
            max_ocf_records: self.avro_options.max_ocf_records,
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
            schema: value.schema.clone(),
        }
    }
}
/// Apache Avro serializer options.
#[configurable_component]
#[derive(Clone, Debug)]
pub struct AvroDeserializerOptions {
    /// The Avro schema definition.
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
    pub schema: String,

    /// For Avro datum encoded in Kafka messages, the bytes are prefixed with the schema ID.  Set this to `true` to strip the schema ID prefix, as described in [Confluent Kafka's documentation](https://docs.confluent.io/platform/current/schema-registry/fundamentals/serdes-develop/index.html#wire-format).
    pub strip_schema_id_prefix: bool,

    /// The encoding format to use for decoding.
    ///
    /// Defaults to `datum` for backward compatibility.
    #[serde(default)]
    pub encoding: AvroEncoding,

    /// How to handle the Avro schema for decoding.
    ///
    /// Defaults to `provided` for `datum` encoding and `embedded` for
    /// `object_container_file` encoding.
    #[serde(default)]
    pub schema_source: Option<AvroSchemaSource>,

    /// Maximum number of records decoded from a single Avro Object Container File.
    ///
    /// Applies only when `encoding` is `object_container_file`.
    #[serde(default = "default_max_ocf_records")]
    pub max_ocf_records: usize,
}

const fn default_max_ocf_records() -> usize {
    DEFAULT_MAX_OCF_RECORDS
}

// Note on framing for `object_container_file` encoding:
// The OCF decoder (`parse_ocf`) requires that each call receives a complete, self-contained OCF
// payload (header + all data blocks). Framers that split on newlines or other delimiters (the
// default for most sources) will produce incomplete buffers and fail to parse.
// Use a framer that delivers whole OCF files, such as:
//   - `length_delimited` (if the upstream writes length-prefixed OCF blobs)
//   - `bytes` (for sources that deliver one complete OCF per message, e.g. S3 objects)
// Do NOT use `newline_delimited` framing with OCF encoding.

/// Serializer that converts bytes to an `Event` using the Apache Avro format.
#[derive(Debug, Clone)]
pub struct AvroDeserializer {
    schema: Option<apache_avro::Schema>,
    strip_schema_id_prefix: bool,
    encoding: AvroEncoding,
    schema_source: AvroSchemaSource,
    max_ocf_bytes: usize,
    max_ocf_records: usize,
}

impl AvroDeserializer {
    /// Creates a new `AvroDeserializer`.
    pub fn new(
        schema: Option<apache_avro::Schema>,
        strip_schema_id_prefix: bool,
        encoding: AvroEncoding,
        schema_source: AvroSchemaSource,
    ) -> Self {
        Self {
            schema,
            strip_schema_id_prefix,
            encoding,
            schema_source,
            max_ocf_bytes: max_decompressed_size_bytes(),
            max_ocf_records: DEFAULT_MAX_OCF_RECORDS,
        }
    }

    fn parse_datum(
        &self,
        bytes: Bytes,
        log_namespace: LogNamespace,
    ) -> vector_common::Result<SmallVec<[Event; 1]>> {
        // Avro has a `null` type which indicates no value.
        if bytes.is_empty() {
            return Ok(smallvec![]);
        }

        let bytes = if self.strip_schema_id_prefix {
            if bytes.len() >= CONFLUENT_SCHEMA_PREFIX_LEN && bytes[0] == CONFLUENT_MAGIC_BYTE {
                bytes.slice(CONFLUENT_SCHEMA_PREFIX_LEN..)
            } else {
                return Err(vector_common::Error::from(
                    "Expected avro datum to be prefixed with schema id",
                ));
            }
        } else {
            bytes
        };

        let schema = self
            .schema
            .as_ref()
            .ok_or_else(|| vector_common::Error::from("Schema required for datum decoding"))?;
        let value = apache_avro::reader::datum::GenericDatumReader::builder(schema)
            .build()?
            .read_value(&mut bytes.reader())?;

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
                        let timestamp = Utc::now();
                        log.insert(timestamp_key, timestamp);
                    }
                }
                event
            }
        };
        Ok(smallvec![event])
    }

    fn parse_ocf(
        &self,
        bytes: Bytes,
        log_namespace: LogNamespace,
    ) -> vector_common::Result<SmallVec<[Event; 1]>> {
        if bytes.is_empty() {
            return Ok(smallvec![]);
        }

        if bytes.len() > self.max_ocf_bytes {
            return Err(vector_common::Error::from(format!(
                "OCF payload size {} bytes exceeds configured max_ocf_bytes of {} bytes",
                bytes.len(),
                self.max_ocf_bytes
            )));
        }

        let binding = bytes.reader();
        let reader = apache_avro::Reader::new(binding)?;
        let embedded_schema = reader.writer_schema().clone();

        // Validate schema using Rabin fingerprint comparison (per Avro spec).
        // Using PartialEq on apache_avro::Schema is fragile because:
        // - Fully-qualified names may differ between user-provided JSON and the OCF's stored form
        // - Schema equality semantics have changed across apache-avro releases
        if self.schema_source == AvroSchemaSource::Provided
            && let Some(provided_schema) = &self.schema
        {
            use apache_avro::rabin::Rabin;
            if provided_schema.fingerprint::<Rabin>().bytes
                != embedded_schema.fingerprint::<Rabin>().bytes
            {
                return Err(vector_common::Error::from(
                    "Embedded schema fingerprint does not match provided schema",
                ));
            }
        }

        let mut events = SmallVec::new();
        for value in reader {
            if events.len() >= self.max_ocf_records {
                return Err(vector_common::Error::from(format!(
                    "OCF record count exceeds configured max_ocf_records of {}",
                    self.max_ocf_records
                )));
            }

            let value = value?;
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
                            let timestamp = Utc::now();
                            log.insert(timestamp_key, timestamp);
                        }
                    }
                    event
                }
            };
            events.push(event);
        }

        Ok(events)
    }
}

impl Deserializer for AvroDeserializer {
    fn parse(
        &self,
        bytes: Bytes,
        log_namespace: LogNamespace,
    ) -> vector_common::Result<SmallVec<[Event; 1]>> {
        match self.encoding {
            AvroEncoding::Datum => self.parse_datum(bytes, log_namespace),
            AvroEncoding::ObjectContainerFile => self.parse_ocf(bytes, log_namespace),
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

    fn ocf_bytes(records: &[Log]) -> Bytes {
        let schema = get_schema();
        let mut writer = apache_avro::Writer::new(&schema, Vec::new()).unwrap();

        for record in records {
            writer
                .append_value(apache_avro::to_value(record.clone()).unwrap())
                .unwrap();
        }

        Bytes::from(writer.into_inner().unwrap())
    }

    fn datum_bytes(schema: &Schema, value: AvroValue) -> Vec<u8> {
        apache_avro::writer::datum::GenericDatumWriter::builder(schema)
            .build()
            .unwrap()
            .write_value_to_vec(value)
            .unwrap()
    }

    fn ocf_deserializer(max_ocf_bytes: usize, max_ocf_records: usize) -> AvroDeserializer {
        AvroDeserializer {
            schema: None,
            strip_schema_id_prefix: false,
            encoding: AvroEncoding::ObjectContainerFile,
            schema_source: AvroSchemaSource::Embedded,
            max_ocf_bytes,
            max_ocf_records,
        }
    }

    #[test]
    fn schema_source_defaults_by_encoding() {
        let datum = AvroDeserializerConfig::new(get_schema().canonical_form(), false)
            .build()
            .expect("datum configuration should build");
        assert_eq!(datum.schema_source, AvroSchemaSource::Provided);
        assert!(datum.schema.is_some());

        let ocf = AvroDeserializerConfig {
            avro_options: AvroDeserializerOptions {
                schema: "not valid Avro schema".to_owned(),
                strip_schema_id_prefix: false,
                encoding: AvroEncoding::ObjectContainerFile,
                schema_source: None,
                max_ocf_records: DEFAULT_MAX_OCF_RECORDS,
            },
        }
        .build()
        .expect("OCF configuration should use its embedded schema by default");
        assert_eq!(ocf.schema_source, AvroSchemaSource::Embedded);
        assert!(ocf.schema.is_none());
    }

    #[test]
    fn provided_ocf_schema_must_match_embedded_schema() {
        let config = AvroDeserializerConfig::new_with_options(
            r#"{"type":"record","name":"log","fields":[{"name":"different","type":"string"}]}"#
                .to_owned(),
            false,
            AvroEncoding::ObjectContainerFile,
            AvroSchemaSource::Provided,
        );
        let deserializer = config
            .build()
            .expect("provided schema should parse during configuration build");

        let error = deserializer
            .parse(
                ocf_bytes(&[Log {
                    message: "hello".to_owned(),
                }]),
                LogNamespace::Vector,
            )
            .expect_err("mismatched provided schema must be rejected");
        assert!(error.to_string().contains("fingerprint"));
    }

    #[test]
    fn deserialize_avro() {
        let schema = get_schema();

        let event = Log {
            message: "hello from avro".to_owned(),
        };
        let record_value = apache_avro::to_value(event).unwrap();
        let record_datum = datum_bytes(&schema, record_value);
        let record_bytes = Bytes::from(record_datum);

        let deserializer = AvroDeserializer::new(
            Some(schema),
            false,
            AvroEncoding::Datum,
            AvroSchemaSource::Provided,
        );
        let events = deserializer
            .parse(record_bytes, LogNamespace::Vector)
            .unwrap();
        assert_eq!(events.len(), 1);

        assert_eq!(
            events[0].as_log().get(event_path!("message")).unwrap(),
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
        let record_datum = datum_bytes(&schema, record_value);

        let mut bytes = BytesMut::new();
        bytes.extend([0, 0, 0, 0, 0]); // 0 prefix + 4 byte schema id
        bytes.extend(record_datum);

        let deserializer = AvroDeserializer::new(
            Some(schema),
            true,
            AvroEncoding::Datum,
            AvroSchemaSource::Provided,
        );
        let events = deserializer
            .parse(bytes.freeze(), LogNamespace::Vector)
            .unwrap();
        assert_eq!(events.len(), 1);

        assert_eq!(
            events[0].as_log().get(event_path!("message")).unwrap(),
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
        let datum = datum_bytes(&schema, value);

        let mut bytes = BytesMut::new();
        bytes.extend([0, 0, 0, 0, 0]); // 0 prefix + 4 byte schema id
        bytes.extend(datum);

        let deserializer = AvroDeserializer::new(
            Some(schema),
            true,
            AvroEncoding::Datum,
            AvroSchemaSource::Provided,
        );
        let events = deserializer
            .parse(bytes.freeze(), LogNamespace::Vector)
            .unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(
            events[0].as_log().get(event_path!("message")).unwrap(),
            &VrlValue::from(uuid)
        );
    }

    #[test]
    fn deserialize_avro_ocf() {
        use std::io::Write;
        use tempfile::NamedTempFile;

        let schema = get_schema();

        // Create test data and write to OCF file
        let records = vec![
            Log {
                message: "first message".to_owned(),
            },
            Log {
                message: "second message".to_owned(),
            },
            Log {
                message: "third message".to_owned(),
            },
        ];

        // Write OCF file using apache_avro library
        let mut ocf_file = NamedTempFile::new().unwrap();
        let mut writer = apache_avro::Writer::new(&schema, Vec::new()).unwrap();

        for record in &records {
            let record_value = apache_avro::to_value(record.clone()).unwrap();
            writer.append_value(record_value).unwrap();
        }

        let ocf_data = writer.into_inner().unwrap();
        ocf_file.write_all(&ocf_data).unwrap();
        ocf_file.flush().unwrap();

        // Now test the deserializer with OCF encoding
        let ocf_bytes = std::fs::read(ocf_file.path()).unwrap();

        // Use the AvroDeserializer to parse the OCF file
        let deserializer = AvroDeserializer::new(
            None, // No schema needed for OCF with embedded schema
            false,
            AvroEncoding::ObjectContainerFile,
            AvroSchemaSource::Embedded,
        );

        let events = deserializer
            .parse(Bytes::from(ocf_bytes), LogNamespace::Vector)
            .unwrap();

        // Validate that all 3 records were deserialized
        assert_eq!(events.len(), 3);
        assert_eq!(
            events[0].as_log().get(event_path!("message")).unwrap(),
            &VrlValue::from("first message")
        );
        assert_eq!(
            events[1].as_log().get(event_path!("message")).unwrap(),
            &VrlValue::from("second message")
        );
        assert_eq!(
            events[2].as_log().get(event_path!("message")).unwrap(),
            &VrlValue::from("third message")
        );
    }

    #[test]
    fn avro_ocf_uses_global_decompressed_size_limit() {
        let deserializer = AvroDeserializer::new(
            None,
            false,
            AvroEncoding::ObjectContainerFile,
            AvroSchemaSource::Embedded,
        );

        assert_eq!(deserializer.max_ocf_bytes, max_decompressed_size_bytes());
    }

    #[test]
    fn deserialize_avro_ocf_rejects_oversized_payload() {
        let bytes = ocf_bytes(&[Log {
            message: "too large".to_owned(),
        }]);
        let deserializer = ocf_deserializer(bytes.len() - 1, DEFAULT_MAX_OCF_RECORDS);

        let error = deserializer
            .parse(bytes, LogNamespace::Vector)
            .expect_err("payload larger than max_ocf_bytes must be rejected");

        assert!(error.to_string().contains("max_ocf_bytes"));
    }

    #[test]
    fn deserialize_avro_ocf_rejects_excessive_records() {
        let bytes = ocf_bytes(&[
            Log {
                message: "first".to_owned(),
            },
            Log {
                message: "second".to_owned(),
            },
        ]);
        let deserializer = ocf_deserializer(max_decompressed_size_bytes(), 1);

        let error = deserializer
            .parse(bytes, LogNamespace::Vector)
            .expect_err("record count larger than max_ocf_records must be rejected");

        assert!(error.to_string().contains("max_ocf_records"));
    }

    #[test]
    fn deserialize_avro_ocf_rejects_truncated_payload() {
        let error = ocf_deserializer(max_decompressed_size_bytes(), DEFAULT_MAX_OCF_RECORDS)
            .parse(Bytes::from_static(b"Obj\x01"), LogNamespace::Vector)
            .expect_err("truncated OCF payload must be rejected");

        assert!(!error.to_string().is_empty());
    }

    #[test]
    fn avro_ocf_build_rejects_zero_limits() {
        let mut config = AvroDeserializerConfig::new_with_options(
            get_schema().canonical_form(),
            false,
            AvroEncoding::ObjectContainerFile,
            AvroSchemaSource::Embedded,
        );
        config.avro_options.max_ocf_records = 0;

        let error = config
            .build()
            .expect_err("zero OCF record limit must be rejected during configuration build");

        assert!(error.to_string().contains("max_ocf_records"));
    }
}
