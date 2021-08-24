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
use chrono::Utc;
use futures::{future::BoxFuture, stream, FutureExt, SinkExt, StreamExt};
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

        // let filename_time_format = "";
        // let filename_append_uuid = self.filename_append_uuid.unwrap_or(true);
        let batch = BatchSettings::default().bytes(10_000_000).timeout(300);

        let s3 = S3Sink { client };
        //
        // let filename_extension = self.filename_extension.clone();
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

fn encode_event(mut event: Event) -> Option<EncodedEvent<PartitionInnerBuffer<Vec<u8>, Bytes>>> {
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

    // TODO generate id, rename fields

    let mut log = event.into_log();
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
    use vector_core::event::LogEvent;

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
        assert_eq!(
            encoded_json,
            serde_json::from_str(
                r#"{ "message": "test message", "timestamp": "2021-08-23T16:00:27.879Z" } "#
            )
            .unwrap()
        );
        assert_eq!(key, "/dt=20210823/hour=16/archive_160027.879");
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
