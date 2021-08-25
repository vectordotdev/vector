use crate::sinks::aws_s3::Request;
use crate::sinks::util::ServiceBuilderExt;
use crate::{
    config::{DataType, SinkConfig, SinkContext},
    internal_events::{aws_s3::sink::S3EventsSent, TemplateRenderingFailed},
    rusoto,
    rusoto::{AwsAuthentication, RegionOrEndpoint},
    sinks::{
        aws_s3,
        aws_s3::S3RetryLogic,
        aws_s3::{
            Encoding, S3CannedAcl, S3ServerSideEncryption, S3Sink, S3SinkConfig, S3StorageClass,
        },
        util::encoding::EncodingConfig,
        util::BatchSettings,
        util::{
            BatchConfig, Buffer, Compression, Concurrency, EncodedEvent, PartitionBatchSink,
            PartitionBuffer, PartitionInnerBuffer, TowerRequestConfig,
        },
    },
    template::Template,
};
use bytes::Bytes;
use bytes::{BufMut, BytesMut};
use chrono::Utc;
use futures::{future::BoxFuture, stream, FutureExt, SinkExt, StreamExt};
use global_counter::generic::Counter;
use global_counter::primitive::exact::CounterU32;
use http::StatusCode;
use lazy_static::lazy_static;
use rusoto_core::RusotoError;
use rusoto_s3::{HeadBucketRequest, S3Client, S3};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    convert::{TryFrom, TryInto},
};
use tower::{Service, ServiceBuilder};
use uuid::Uuid;
use vector_core::config::proxy::ProxyConfig;
use vector_core::event::Event;

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct DatadogArchivesSinkConfig {
    pub service: String,
    pub bucket: String,
    pub key_prefix: Option<String>,
    #[serde(default)]
    pub request: TowerRequestConfig,
    #[serde(default)]
    pub aws_s3_config: Option<S3Config>,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
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

impl DatadogArchivesSinkConfig {
    pub fn new(&self, client: S3Client, cx: SinkContext) -> crate::Result<super::VectorSink> {
        let request = self.request.unwrap_with(&TowerRequestConfig {
            concurrency: Concurrency::Fixed(50),
            rate_limit_num: Some(250),
            ..Default::default()
        });

        let batch = BatchSettings::default().bytes(10_000_000).timeout(300);

        let s3 = S3Sink { client };
        //
        let s3_options = self
            .aws_s3_config
            .as_ref()
            .and_then(|c| Some(c.options.clone()))
            .expect("s3 config wasn't provided");

        let bucket = self.bucket.clone();
        let prefix = self.key_prefix.clone();

        let svc = ServiceBuilder::new()
            .map(move |req| {
                build_s3_request(req, bucket.clone(), prefix.clone(), s3_options.clone())
            })
            .settings(request, S3RetryLogic)
            .service(s3);

        let buffer = PartitionBuffer::new(Buffer::new(batch.size, Compression::gzip_default()));

        let sink = PartitionBatchSink::new(svc, buffer, batch.timeout, cx.acker())
            .with_flat_map(move |e| stream::iter(encode_event(e)).map(Ok))
            .sink_map_err(|error| error!(message = "Sink failed to flush.", %error));

        Ok(super::VectorSink::Sink(Box::new(sink)))
    }
}

fn build_s3_request(
    req: PartitionInnerBuffer<Vec<u8>, Bytes>,
    bucket: String,
    path_prefix: Option<String>,
    options: S3Options,
) -> Request {
    let (inner, key) = req.into_parts();

    let filename = Uuid::new_v4().to_string();

    let key = String::from_utf8_lossy(&key[..]).into_owned();
    let key = format!(
        "{}/{}.{}.{}",
        path_prefix.unwrap_or_default(),
        key,
        filename,
        "json.gz"
    )
    .replace("//", "/");

    debug!(
        message = "Sending events.",
        bytes = ?inner.len(),
        bucket = ?bucket,
        key = ?key
    );

    Request {
        body: inner,
        bucket,
        key: key.to_string(),
        content_encoding: Compression::gzip_default().content_encoding(),
        options: aws_s3::S3Options {
            acl: options.acl,
            grant_full_control: options.grant_full_control,
            grant_read: options.grant_read,
            grant_read_acp: options.grant_read_acp,
            grant_write_acp: options.grant_write_acp,
            server_side_encryption: options.server_side_encryption,
            ssekms_key_id: options.ssekms_key_id,
            storage_class: options.storage_class,
            tags: options.tags,
            content_encoding: Some("gzip".to_string()),
            content_type: Some("text/json".to_string()),
        },
    }
}

fn encode_event(event: Event) -> Option<EncodedEvent<PartitionInnerBuffer<Vec<u8>, Bytes>>> {
    lazy_static! {
        static ref KEY_PREFIX: Template =
            Template::try_from("/dt=%Y%m%d/hour=%H/archive_%H%M%S%.3f")
                .expect("invalid object key format");
    }
    let key = KEY_PREFIX
        .render_string(&event)
        .map_err(|error| {
            emit!(TemplateRenderingFailed {
                error,
                field: Some("key_prefix"),
                drop_event: true,
            });
        })
        .ok()?;

    let mut log = event.into_log();

    log.insert("_id", generate_log_id());

    let bytes = serde_json::to_vec(&log)
        .map(|mut b| {
            b.push(b'\n');
            b
        })
        .expect("Failed to encode event as json, this is a bug!");

    Some(EncodedEvent {
        item: PartitionInnerBuffer::new(bytes, key.into()), //TODO
        finalizers: log.metadata_mut().take_finalizers(),
    })
}

fn generate_log_id() -> String {
    let mut id = BytesMut::with_capacity(18);
    // timestamp in millis - 6 bytes
    let now = Utc::now();
    id.put_int(now.timestamp_millis(), 6);
    // namespace + version, both 0 - 2 bytes
    id.put_i16(0);

    // host-based number - 5 bytes
    lazy_static! {
        static ref HOST_NUMBER: Vec<u8> = host_number();
    }
    id.put_slice(&HOST_NUMBER[..]);

    // counter - 5 bytes
    id.put_u8(0); // one padding byte
                  // 4 bytes for the counter should be more than enough - it should be unique for 1 millisecond only
    lazy_static! {
        static ref COUNTER: CounterU32 = CounterU32::new(0);
    }
    id.put_u32(COUNTER.inc()); // 4 bytes

    base64::encode(id.freeze())
}

/// returns 5 first bytes of the MAC address XOR-ed with random bytes
fn host_number() -> Vec<u8> {
    extern crate mac_address;
    use mac_address::get_mac_address;
    use rand::Rng;

    let mac = get_mac_address().unwrap().unwrap().bytes();
    let mut rng = rand::thread_rng();
    let mut rand_bytes = rng.gen::<[u8; 5]>();

    rand_bytes
        .iter_mut()
        .zip(mac[..5].iter()) // take first 5 bytes
        .map(|(x, y)| *x ^ *y) // apply XOR
        .collect::<Vec<_>>()
}

#[async_trait::async_trait]
#[typetag::serde(name = "datadog_archives")]
impl SinkConfig for DatadogArchivesSinkConfig {
    async fn build(
        &self,
        cx: SinkContext,
    ) -> crate::Result<(super::VectorSink, super::Healthcheck)> {
        let s3config = self.aws_s3_config.as_ref().expect("TODO");
        let client = aws_s3::create_client(&s3config.region, &s3config.auth, &cx.proxy)?;
        let healthcheck = aws_s3::healthcheck(self.bucket.clone(), client.clone()).boxed();
        let sink = self.new(client, cx)?;
        Ok((sink, healthcheck))
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
    use chrono::DateTime;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<S3SinkConfig>();
    }

    #[test]
    fn s3_encode_event_text() {
        let mut log = Event::from("test message");
        let timestamp = DateTime::parse_from_rfc3339("2021-08-23T18:00:27.879+02:00")
            .expect("invalid test case")
            .with_timezone(&Utc);
        log.as_mut_log().insert("timestamp", timestamp);
        let encoded = encode_event(log).unwrap();
        let (bytes, key) = encoded.item.into_parts();
        let encoded_json: BTreeMap<String, String> = serde_json::from_slice(&bytes[..]).unwrap();

        let id1 = validate_event_id(&encoded_json);

        assert_eq!(
            encoded_json.get("message").expect("message not found"),
            "test message"
        );
        assert_eq!(
            encoded_json.get("timestamp").expect("timestamp not found"),
            "2021-08-23T16:00:27.879Z"
        );
        assert_eq!(key, "/dt=20210823/hour=16/archive_160027.879");

        // check that id is different
        let encoded = encode_event(Event::new_empty_log()).unwrap();
        let (bytes, _) = encoded.item.into_parts();
        let encoded_json: BTreeMap<String, String> = serde_json::from_slice(&bytes[..]).unwrap();

        let id2 = validate_event_id(&encoded_json);
        assert_ne!(id1, id2)
    }

    /// check that _id is 18 bytes, base64-encoded, first 6 bytes - a "now" timestamp in millis
    fn validate_event_id(encoded_json: &BTreeMap<String, String>) -> String {
        let id = encoded_json.get("_id").expect("_id not found");
        let bytes = base64::decode(id).expect("_id is not base64-encoded");
        assert_eq!(bytes.len(), 18);
        let mut timestamp: [u8; 8] = [0; 8];
        for (i, b) in bytes[..6].iter().enumerate() {
            timestamp[i + 2] = *b;
        }
        let timestamp = i64::from_be_bytes(timestamp);
        // check that it is a recent timestamp in millis
        assert!(Utc::now().timestamp_millis() - timestamp < 1000);
        id.to_owned()
    }

    #[test]
    fn s3_build_request() {
        let mut log = Event::from("test message");
        let timestamp = DateTime::parse_from_rfc3339("2021-08-23T18:00:27.879+02:00")
            .expect("invalid test case")
            .with_timezone(&Utc);
        log.as_mut_log().insert("timestamp", timestamp);
        let encoded = encode_event(log).unwrap();
        // let buf = PartitionInnerBuffer::new(vec![0u8; 10], Bytes::from("key/"));

        let req = build_s3_request(
            encoded.item,
            "dd-logs".into(),
            Some("audit".into()),
            S3Options::default(),
        );
        let prefix = "audit/dt=20210823/hour=16/archive_160027.879.";
        let file_ext = ".json.gz";
        assert!(req.key.starts_with(prefix));
        assert!(req.key.ends_with(file_ext));
        let uuid1 = &req.key[prefix.len()..req.key.len() - file_ext.len()];
        assert_eq!(uuid1.len(), 36);

        // check the the second batch has a different UUID
        let encoded = encode_event(Event::new_empty_log()).unwrap();
        let req = build_s3_request(
            encoded.item,
            "dd-logs".into(),
            Some("audit".into()),
            S3Options::default(),
        );
        let uuid2 = &req.key[prefix.len()..req.key.len() - file_ext.len()];
        assert_ne!(uuid1, uuid2);
    }
}
