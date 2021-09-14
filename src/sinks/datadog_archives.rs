use std::{
    collections::BTreeMap,
    collections::HashSet,
    convert::TryFrom,
    io::{self, Write},
    num::NonZeroUsize,
    time::Duration,
};

use bytes::{BufMut, BytesMut};
use chrono::{SecondsFormat, Utc};
use rand::{thread_rng, Rng};
use rusoto_s3::S3Client;
use serde::{Deserialize, Serialize};
use snafu::Snafu;
use tower::ServiceBuilder;
use uuid::Uuid;

use vector_core::event::Event;

use crate::sinks::s3_common;
use crate::{
    config::GenerateConfig,
    config::{DataType, SinkConfig, SinkContext},
    event::PathComponent,
    rusoto::{AwsAuthentication, RegionOrEndpoint},
    sinks::{
        s3_common::{
            config::{
                build_healthcheck, create_client, S3CannedAcl, S3RetryLogic,
                S3ServerSideEncryption, S3StorageClass,
            },
            partitioner::KeyPartitioner,
            service::{S3Request, S3Service},
            sink::{process_event_batch, S3EventEncoding, S3RequestBuilder, S3Sink},
        },
        util::encoding::{EncodingConfiguration, TimestampFormat},
        util::Concurrency,
        util::{Compression, ServiceBuilderExt, TowerRequestConfig},
        VectorSink,
    },
    template::Template,
};

const DEFAULT_REQUEST_LIMITS: TowerRequestConfig = {
    TowerRequestConfig::const_new(Concurrency::Fixed(50), Concurrency::Fixed(50))
        .rate_limit_num(250)
};

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct DatadogArchivesSinkConfig {
    pub service: String,
    pub bucket: String,
    pub key_prefix: Option<String>,
    #[serde(default)]
    pub request: TowerRequestConfig,
    #[serde(default)]
    pub aws_s3: Option<S3Config>,
}

#[derive(Deserialize, Serialize, Default, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct S3Config {
    #[serde(flatten)]
    pub options: S3Options,
    #[serde(flatten)]
    pub region: RegionOrEndpoint,
    #[serde(default)]
    pub auth: AwsAuthentication,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct S3Options {
    acl: Option<S3CannedAcl>,
    grant_full_control: Option<String>,
    grant_read: Option<String>,
    grant_read_acp: Option<String>,
    grant_write_acp: Option<String>,
    server_side_encryption: Option<S3ServerSideEncryption>,
    ssekms_key_id: Option<String>,
    storage_class: Option<S3StorageClass>,
    tags: Option<BTreeMap<String, String>>,
}

impl GenerateConfig for DatadogArchivesSinkConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(Self {
            service: "".to_owned(),
            bucket: "".to_owned(),
            key_prefix: None,
            request: TowerRequestConfig::default(),
            aws_s3: Some(S3Config::default()),
        })
        .unwrap()
    }
}

#[derive(Debug, Snafu, PartialEq)]
enum ConfigError {
    #[snafu(display("Unsupported service: {}", service))]
    UnsupportedService { service: String },
    #[snafu(display("Unsupported storage class: {}", storage_class))]
    UnsupportedStorageClass { storage_class: String },
}

const KEY_TEMPLATE: &str = "/dt=%Y%m%d/hour=%H/";

impl DatadogArchivesSinkConfig {
    fn new(&self, cx: SinkContext) -> crate::Result<(super::VectorSink, super::Healthcheck)> {
        match &self.service[..] {
            "aws_s3" => {
                let s3_config = self.aws_s3.as_ref().expect("s3 config wasn't provided");
                let client = create_client(&s3_config.region, &s3_config.auth, None, &cx.proxy)?;
                let svc = self
                    .build_s3_sink(&s3_config.options, client.clone(), cx)
                    .map_err(|error| format!("{}", error))?;
                Ok((svc, build_healthcheck(self.bucket.clone(), client)?))
            }

            service => Err(Box::new(ConfigError::UnsupportedService {
                service: service.to_owned(),
            })),
        }
    }

    fn build_s3_sink(
        &self,
        s3_options: &S3Options,
        client: S3Client,
        cx: SinkContext,
    ) -> std::result::Result<VectorSink, ConfigError> {
        // we use lower default limits, because we send 100mb batches,
        // thus no need in the the higher number of outcoming requests
        let request_limits = self.request.unwrap_with(&DEFAULT_REQUEST_LIMITS);
        let service = ServiceBuilder::new()
            .settings(request_limits, S3RetryLogic)
            .service(S3Service::new(client));

        match s3_options.storage_class {
            Some(class @ S3StorageClass::DeepArchive) | Some(class @ S3StorageClass::Glacier) => {
                return Err(ConfigError::UnsupportedStorageClass {
                    storage_class: format!("{:?}", class),
                });
            }
            _ => (),
        }

        // We should avoid producing many small batches - this might slow down Log Rehydration,
        // these values are similar with how DataDog's Log Archives work internally:
        // batch size - 100mb
        // batch timeout - 15min
        let batch_size_bytes = NonZeroUsize::new(100_000_000);
        let batch_size_events =
            NonZeroUsize::new(200_000).expect("batch size, in events, must be greater than 0"); // provided the average log size is around 500 bytes
        let batch_timeout = Duration::from_secs(900);

        let partitioner = DatadogArchivesSinkConfig::key_partitioner();

        let request_builder = DatadogS3RequestBuilder {
            bucket: self.bucket.clone(),
            key_prefix: self.key_prefix.clone(),
            aws_s3: self
                .aws_s3
                .as_ref()
                .expect("s3 config wasn't provided")
                .clone(),
        };

        let sink = S3Sink::new(
            cx,
            service,
            request_builder,
            partitioner,
            batch_size_bytes,
            batch_size_events,
            batch_timeout,
        );

        Ok(VectorSink::Stream(Box::new(sink)))
    }

    fn key_partitioner() -> KeyPartitioner {
        KeyPartitioner::new(Template::try_from(KEY_TEMPLATE).expect("invalid object key format"))
    }
}

const RESERVED_ATTRIBUTES: [&str; 10] = [
    "_id", "date", "message", "host", "source", "service", "status", "tags", "trace_id", "span_id",
];

#[derive(Debug, Clone)]
struct DatadogArchivesSinkEncoding {
    reserved_attributes: HashSet<&'static str>,
    id_rnd_bytes: [u8; 8],
    id_seq_number: u32,
}

impl S3EventEncoding for DatadogArchivesSinkEncoding {
    /// Applies the following transformations to align event's schema with DD:
    /// - `_id` is generated in the sink(format described below);
    /// - `date` is set from the Global Log Schema's `timestamp` mapping, or to the current time if missing;
    /// - `message`,`host` are set from the corresponding Global Log Schema mappings;
    /// - `source`, `service`, `status`, `tags` and other reserved attributes are left as is;
    /// - the rest of the fields is moved to `attributes`.
    fn encode_event(&mut self, event: Event, mut writer: &mut dyn Write) -> io::Result<()> {
        let mut log_event = event.into_log();

        log_event.insert("_id", self.generate_log_id());

        let timestamp = log_event
            .remove(crate::config::log_schema().timestamp_key())
            .unwrap_or_else(|| chrono::Utc::now().timestamp_millis().into());
        log_event.insert(
            "date",
            timestamp
                .as_timestamp()
                .cloned()
                .unwrap_or_else(chrono::Utc::now)
                .to_rfc3339_opts(SecondsFormat::Millis, true),
        );
        log_event.rename_key_flat(crate::config::log_schema().message_key(), "message");
        log_event.rename_key_flat(crate::config::log_schema().host_key(), "host");

        let mut attributes = BTreeMap::new();
        let custom_attributes: Vec<String> = log_event
            .keys()
            .filter(|path| !self.reserved_attributes.contains(path.as_str()))
            .collect();
        for path in custom_attributes {
            if let Some(value) = log_event.remove(&path) {
                attributes.insert(path, value);
            }
        }
        log_event.insert("attributes", attributes);

        let _ = serde_json::to_writer(&mut writer, &log_event)?;
        writer.write_all(b"\n")
    }
}

impl DatadogArchivesSinkEncoding {
    /// Generates a unique event ID compatible with DD:
    /// - 18 bytes;
    /// - first 6 bytes represent a "now" timestamp in millis;
    /// - the rest 12 bytes can be just any sequence unique for a given timestamp.
    ///
    /// To generate unique-ish trailing 12 bytes we use random 8 bytes, generated at startup,
    /// and a rolling-over 4-bytes sequence number.
    fn generate_log_id(&mut self) -> String {
        let mut id = BytesMut::with_capacity(18);
        // timestamp in millis - 6 bytes
        let now = Utc::now();
        id.put_int(now.timestamp_millis(), 6);

        // 8 random bytes
        id.put_slice(&self.id_rnd_bytes);

        // 4 bytes for the counter should be more than enough - it should be unique for 1 millisecond only
        self.id_seq_number = self.id_seq_number.wrapping_add(1);
        id.put_u32(self.id_seq_number);

        base64::encode(id.freeze())
    }
}

fn default_encoding() -> DatadogArchivesSinkEncoding {
    DatadogArchivesSinkEncoding {
        reserved_attributes: RESERVED_ATTRIBUTES.to_vec().into_iter().collect(),
        id_rnd_bytes: thread_rng().gen::<[u8; 8]>(),
        id_seq_number: u32::default(),
    }
}

#[derive(Debug, Clone)]
struct DatadogS3RequestBuilder {
    pub bucket: String,
    pub key_prefix: Option<String>,
    pub aws_s3: S3Config,
}

impl S3RequestBuilder for DatadogS3RequestBuilder {
    fn build_request(&mut self, key: String, batch: Vec<Event>) -> S3Request {
        let filename = Uuid::new_v4().to_string();

        let key = format!(
            "{}/{}{}.{}",
            self.key_prefix.clone().unwrap_or_default(),
            key,
            filename,
            "json.gz"
        )
        .replace("//", "/");

        let batch_size = batch.len();
        let (body, finalizers) =
            process_event_batch(batch, &mut default_encoding(), Compression::gzip_default());

        debug!(
            message = "Sending events.",
            bytes = ?body.len(),
            bucket = ?self.bucket,
            key = ?key
        );

        let s3_options = self.aws_s3.options.clone();
        S3Request {
            body,
            bucket: self.bucket.clone(),
            key,
            content_encoding: Compression::gzip_default().content_encoding(),
            options: s3_common::config::S3Options {
                acl: s3_options.acl,
                grant_full_control: s3_options.grant_full_control,
                grant_read: s3_options.grant_read,
                grant_read_acp: s3_options.grant_read_acp,
                grant_write_acp: s3_options.grant_write_acp,
                server_side_encryption: s3_options.server_side_encryption,
                ssekms_key_id: s3_options.ssekms_key_id,
                storage_class: s3_options.storage_class,
                tags: s3_options.tags,
                content_encoding: None,
                content_type: None,
            },
            batch_size,
            finalizers,
        }
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "datadog_archives")]
impl SinkConfig for DatadogArchivesSinkConfig {
    async fn build(
        &self,
        cx: SinkContext,
    ) -> crate::Result<(super::VectorSink, super::Healthcheck)> {
        let sink_and_healthcheck = self.new(cx)?;
        Ok(sink_and_healthcheck)
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }

    fn sink_type(&self) -> &'static str {
        "datadog_archives"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::LogEvent;
    use chrono::DateTime;
    use std::{collections::BTreeMap, io::Cursor};
    use vector_core::partition::Partitioner;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<DatadogArchivesSinkConfig>();
    }

    #[test]
    fn encodes_event() {
        let mut event = Event::from("test message");
        let log_mut = event.as_mut_log();
        log_mut.insert("service", "test-service");
        log_mut.insert("not_a_reserved_attribute", "value");
        let timestamp = DateTime::parse_from_rfc3339("2021-08-23T18:00:27.879+02:00")
            .expect("invalid test case")
            .with_timezone(&Utc);
        log_mut.insert("timestamp", timestamp);

        let mut writer = Cursor::new(Vec::new());
        let mut encoding = default_encoding();
        encoding.encode_event(event, &mut writer);

        let encoded = writer.into_inner();
        let json: BTreeMap<String, serde_json::Value> =
            serde_json::from_slice(encoded.as_slice()).unwrap();

        validate_event_id(
            &json
                .get("_id")
                .expect("_id not found")
                .as_str()
                .expect("_id is not a string"),
        );

        assert_eq!(json.len(), 5); // _id, message, date, service, attributes
        assert_eq!(
            json.get("message")
                .expect("message not found")
                .as_str()
                .expect("message is not a string"),
            "test message"
        );
        assert_eq!(
            json.get("date")
                .expect("date not found")
                .as_str()
                .expect("date is not a string"),
            "2021-08-23T16:00:27.879Z"
        );
        assert_eq!(
            json.get("service")
                .expect("service not found")
                .as_str()
                .expect("service is not a string"),
            "test-service"
        );

        let attributes = json
            .get("attributes")
            .expect("attributes not found")
            .as_object()
            .expect("attributes is not an object");
        assert_eq!(attributes.len(), 1);
        assert_eq!(
            String::from_utf8_lossy(
                attributes
                    .get("not_a_reserved_attribute")
                    .expect("not_a_reserved_attribute wasn't moved to attributes")
                    .as_str()
                    .expect("not_a_reserved_attribute is not a string")
                    .as_ref()
            ),
            "value"
        );
    }

    #[test]
    fn generates_valid_key_for_an_event() {
        let mut log = LogEvent::from("test message");

        let timestamp = DateTime::parse_from_rfc3339("2021-08-23T18:00:27.879+02:00")
            .expect("invalid test case")
            .with_timezone(&Utc);
        log.insert("timestamp", timestamp);

        let key_partitioner = DatadogArchivesSinkConfig::key_partitioner();
        let key = key_partitioner
            .partition(&log.into())
            .expect("key wasn't provided");

        assert_eq!(key, "/dt=20210823/hour=16/");
    }

    #[test]
    fn generates_valid_id() {
        let log1 = Event::from("test event 1");
        let mut encoding = default_encoding();
        let mut writer = Cursor::new(Vec::new());
        let mut encoding = default_encoding();
        encoding.encode_event(log1, &mut writer);
        let encoded = writer.into_inner();
        let json: BTreeMap<String, serde_json::Value> =
            serde_json::from_slice(encoded.as_slice()).unwrap();
        let id1 = json
            .get("_id")
            .expect("_id not found")
            .as_str()
            .expect("_id is not a string");
        validate_event_id(id1);

        // check that id is different for the next event
        let log2 = Event::from("test event 2");
        let mut writer = Cursor::new(Vec::new());
        encoding.encode_event(log2, &mut writer);
        let encoded = writer.into_inner();
        let json: BTreeMap<String, serde_json::Value> =
            serde_json::from_slice(encoded.as_slice()).unwrap();
        let id2 = json
            .get("_id")
            .expect("_id not found")
            .as_str()
            .expect("_id is not a string");
        validate_event_id(id2);
        assert_ne!(id1, id2)
    }

    #[test]
    fn generates_date_if_missing() {
        let mut log = Event::from("test message");
        let mut writer = Cursor::new(Vec::new());
        let mut encoding = default_encoding();
        encoding.encode_event(log, &mut writer);
        let encoded = writer.into_inner();
        let json: BTreeMap<String, serde_json::Value> =
            serde_json::from_slice(encoded.as_slice()).unwrap();

        let date = DateTime::parse_from_rfc3339(
            json.get("date")
                .expect("date not found")
                .as_str()
                .expect("date is not a string"),
        )
        .expect("date is not in an rfc3339 format");

        // check that it is a recent timestamp
        assert!(Utc::now().timestamp() - date.timestamp() < 1000);
    }

    /// check that _id is:
    /// - 18 bytes,
    /// - base64-encoded,
    /// - first 6 bytes - a "now" timestamp in millis
    fn validate_event_id(id: &str) {
        let bytes = base64::decode(id).expect("_id is not base64-encoded");
        assert_eq!(bytes.len(), 18);
        let mut timestamp: [u8; 8] = [0; 8];
        for (i, b) in bytes[..6].iter().enumerate() {
            timestamp[i + 2] = *b;
        }
        let timestamp = i64::from_be_bytes(timestamp);
        // check that it is a recent timestamp in millis
        assert!(Utc::now().timestamp_millis() - timestamp < 1000);
    }

    #[test]
    fn s3_build_request() {
        let mut log = Event::from("test message");
        let timestamp = DateTime::parse_from_rfc3339("2021-08-23T18:00:27.879+02:00")
            .expect("invalid test case")
            .with_timezone(&Utc);
        log.as_mut_log().insert("timestamp", timestamp);
        let key_partitioner = DatadogArchivesSinkConfig::key_partitioner();
        let key = key_partitioner
            .partition(&log)
            .expect("key wasn't provided");

        let mut request_builder = DatadogS3RequestBuilder {
            bucket: "dd-logs".into(),
            key_prefix: Some("audit".into()),
            aws_s3: S3Config::default(),
        };

        let req = request_builder.build_request(key, vec![log]);
        let expected_key_prefix = "audit/dt=20210823/hour=16/";
        let expected_key_ext = ".json.gz";
        println!("{}", req.key);
        assert!(req.key.starts_with(expected_key_prefix));
        assert!(req.key.ends_with(expected_key_ext));
        let uuid1 = &req.key[expected_key_prefix.len()..req.key.len() - expected_key_ext.len()];
        assert_eq!(uuid1.len(), 36);

        // check the the second batch has a different UUID
        let log2 = Event::new_empty_log();

        let key = key_partitioner
            .partition(&log2)
            .expect("key wasn't provided");
        let req = request_builder.build_request(key, vec![log2]);
        let uuid2 = &req.key[expected_key_prefix.len()..req.key.len() - expected_key_ext.len()];
        assert_ne!(uuid1, uuid2);
    }

    #[tokio::test]
    async fn error_if_unsupported_s3_storage_class() {
        for (class, supported) in [
            (S3StorageClass::Standard, true),
            (S3StorageClass::StandardIa, true),
            (S3StorageClass::IntelligentTiering, true),
            (S3StorageClass::OnezoneIa, true),
            (S3StorageClass::ReducedRedundancy, true),
            (S3StorageClass::DeepArchive, false),
            (S3StorageClass::Glacier, false),
        ] {
            let config = DatadogArchivesSinkConfig {
                service: "aws_s3".to_owned(),
                bucket: "vector-datadog-archives".to_owned(),
                key_prefix: Some("logs/".to_owned()),
                request: TowerRequestConfig::default(),
                aws_s3: Some(S3Config {
                    options: S3Options {
                        storage_class: Some(class),
                        ..Default::default()
                    },
                    region: RegionOrEndpoint::with_region("us-east-1".to_owned()),
                    auth: Default::default(),
                }),
            };

            let res = config.new(SinkContext::new_test());

            if supported {
                assert!(res.is_ok());
            } else {
                assert_eq!(
                    res.err().unwrap().to_string(),
                    format!(r#"Unsupported storage class: {:?}"#, class)
                );
            }
        }
    }
}
