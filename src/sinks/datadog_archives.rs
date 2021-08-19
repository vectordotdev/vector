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
use futures::{future::BoxFuture, stream, FutureExt, SinkExt, StreamExt};
use http::StatusCode;
use rusoto_core::RusotoError;
use rusoto_s3::{HeadBucketRequest, S3Client, S3};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    convert::{TryFrom, TryInto},
};
use tower::{Service, ServiceBuilder};
use vector_core::config::proxy::ProxyConfig;
use vector_core::event::Event;

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct DatadogArchivesSinkConfig {
    pub service: String,
    pub bucket: String,
    pub prefix: Option<String>,
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

        let key_prefix = self.prefix.clone();
        //
        let s3 = S3Sink { client };
        //
        // let filename_extension = self.filename_extension.clone();
        let bucket = self.bucket.clone();
        let s3_options = self
            .aws_s3_config
            .as_ref()
            .and_then(|c| Some(c.options.clone()))
            .expect("s3 config wasn't provided");

        let svc = ServiceBuilder::new()
            .map(move |req| {
                aws_s3::build_request(
                    req,
                    "sdf".to_string(),
                    None,
                    false,
                    Compression::gzip_default(),
                    bucket.clone(),
                    aws_s3::S3Options {
                        acl: s3_options.acl,
                        grant_full_control: s3_options.grant_full_control.clone(),
                        grant_read: s3_options.grant_read.clone(),
                        grant_read_acp: s3_options.grant_read_acp.clone(),
                        grant_write_acp: s3_options.grant_write_acp.clone(),
                        server_side_encryption: s3_options.server_side_encryption.clone(),
                        ssekms_key_id: s3_options.ssekms_key_id.clone(),
                        storage_class: s3_options.storage_class.clone(),
                        tags: s3_options.tags.clone(),
                        content_encoding: None,
                        content_type: None,
                    },
                )
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

fn encode_event(mut event: Event) -> Option<EncodedEvent<PartitionInnerBuffer<Vec<u8>, Bytes>>> {
    // TODO

    // encoding.apply_rules(&mut event);

    let mut log = event.into_log();
    let bytes = serde_json::to_vec(&log)
        .map(|mut b| {
            b.push(b'\n');
            b
        })
        .expect("Failed to encode event as json, this is a bug!");

    Some(EncodedEvent {
        item: PartitionInnerBuffer::new(bytes, Bytes::new()), //TODO
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
        let config = self.aws_s3_config.as_ref().expect("TODO");
        let s3_options = &config.options;
        let s3_sink_config = S3SinkConfig {
            bucket: self.bucket.to_string(),
            key_prefix: None,
            filename_time_format: None,
            filename_append_uuid: None,
            filename_extension: None,
            options: aws_s3::S3Options {
                acl: s3_options.acl,
                grant_full_control: s3_options.grant_full_control.clone(),
                grant_read: s3_options.grant_read.clone(),
                grant_read_acp: s3_options.grant_read_acp.clone(),
                grant_write_acp: s3_options.grant_write_acp.clone(),
                server_side_encryption: s3_options.server_side_encryption.clone(),
                ssekms_key_id: s3_options.ssekms_key_id.clone(),
                storage_class: s3_options.storage_class.clone(),
                tags: s3_options.tags.clone(),
                content_encoding: None,
                content_type: None,
            },
            region: config.region.clone(),
            encoding: aws_s3::Encoding::Ndjson.into(),
            compression: Compression::gzip_default(),
            batch: BatchConfig {
                //TODO
                max_bytes: Some(100_000_000),
                timeout_secs: Some(5),
                ..Default::default()
            },
            request: self.request,
            assume_role: None,
            auth: config.auth.clone(),
        };
        let client = s3_sink_config.create_client(&cx.proxy)?;
        let healthcheck = s3_sink_config.clone().healthcheck(client.clone()).boxed();
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

    #[test]
    fn generate_config() {}
}
