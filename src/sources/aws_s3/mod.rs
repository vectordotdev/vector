use std::{convert::TryInto, io::ErrorKind};

use async_compression::tokio::bufread;
use aws_smithy_types::byte_stream::ByteStream;
use futures::{stream, stream::StreamExt, TryStreamExt};
use snafu::Snafu;
use tokio_util::io::StreamReader;
use vector_lib::codecs::decoding::{
    DeserializerConfig, FramingConfig, NewlineDelimitedDecoderOptions,
};
use vector_lib::codecs::NewlineDelimitedDecoderConfig;
use vector_lib::config::{LegacyKey, LogNamespace};
use vector_lib::configurable::configurable_component;
use vector_lib::lookup::owned_value_path;
use vrl::value::{kind::Collection, Kind};

use super::util::MultilineConfig;
use crate::codecs::DecodingConfig;
use crate::{
    aws::{auth::AwsAuthentication, create_client, create_client_and_region, RegionOrEndpoint},
    common::{s3::S3ClientBuilder, sqs::SqsClientBuilder},
    config::{
        ProxyConfig, SourceAcknowledgementsConfig, SourceConfig, SourceContext, SourceOutput,
    },
    line_agg,
    serde::{bool_or_struct, default_decoding},
    tls::TlsConfig,
};

pub mod sqs;

/// Compression scheme for objects retrieved from S3.
#[configurable_component]
#[configurable(metadata(docs::advanced))]
#[derive(Clone, Copy, Debug, Derivative, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
#[derivative(Default)]
pub enum Compression {
    /// Automatically attempt to determine the compression scheme.
    ///
    /// The compression scheme of the object is determined from its `Content-Encoding` and
    /// `Content-Type` metadata, as well as the key suffix (for example, `.gz`).
    ///
    /// It is set to `none` if the compression scheme cannot be determined.
    #[derivative(Default)]
    Auto,

    /// Uncompressed.
    None,

    /// GZIP.
    Gzip,

    /// ZSTD.
    Zstd,
}

/// Strategies for consuming objects from AWS S3.
#[configurable_component]
#[derive(Clone, Copy, Debug, Derivative)]
#[serde(rename_all = "lowercase")]
#[derivative(Default)]
enum Strategy {
    /// Consumes objects by processing bucket notification events sent to an [AWS SQS queue][aws_sqs].
    ///
    /// [aws_sqs]: https://aws.amazon.com/sqs/
    #[derivative(Default)]
    Sqs,
}

/// Configuration for the `aws_s3` source.
// TODO: The `Default` impl here makes the configuration schema output look pretty weird, especially because all the
// usage of optionals means we're spewing out a ton of `"foo": null` stuff in the default value, and that's not helpful
// when there's required fields.
//
// Maybe showing defaults at all, when there are required properties, doesn't actually make sense? :thinkies:
#[configurable_component(source("aws_s3", "Collect logs from AWS S3."))]
#[derive(Clone, Debug, Derivative)]
#[derivative(Default)]
#[serde(default, deny_unknown_fields)]
pub struct AwsS3Config {
    #[serde(flatten)]
    region: RegionOrEndpoint,

    /// The compression scheme used for decompressing objects retrieved from S3.
    compression: Compression,

    /// The strategy to use to consume objects from S3.
    #[configurable(metadata(docs::hidden))]
    strategy: Strategy,

    /// Configuration options for SQS.
    sqs: Option<sqs::Config>,

    /// The ARN of an [IAM role][iam_role] to assume at startup.
    ///
    /// [iam_role]: https://docs.aws.amazon.com/IAM/latest/UserGuide/id_roles.html
    #[configurable(deprecated)]
    #[configurable(metadata(docs::hidden))]
    assume_role: Option<String>,

    #[configurable(derived)]
    #[serde(default)]
    auth: AwsAuthentication,

    /// Multiline aggregation configuration.
    ///
    /// If not specified, multiline aggregation is disabled.
    #[configurable(derived)]
    multiline: Option<MultilineConfig>,

    #[configurable(derived)]
    #[serde(default, deserialize_with = "bool_or_struct")]
    acknowledgements: SourceAcknowledgementsConfig,

    #[configurable(derived)]
    tls_options: Option<TlsConfig>,

    /// The namespace to use for logs. This overrides the global setting.
    #[configurable(metadata(docs::hidden))]
    #[serde(default)]
    log_namespace: Option<bool>,

    #[configurable(derived)]
    #[serde(default = "default_framing")]
    #[derivative(Default(value = "default_framing()"))]
    pub framing: FramingConfig,

    #[configurable(derived)]
    #[serde(default = "default_decoding")]
    #[derivative(Default(value = "default_decoding()"))]
    pub decoding: DeserializerConfig,
}

const fn default_framing() -> FramingConfig {
    // This is used for backwards compatibility. It used to be the only (hardcoded) option.
    FramingConfig::NewlineDelimited(NewlineDelimitedDecoderConfig {
        newline_delimited: NewlineDelimitedDecoderOptions { max_length: None },
    })
}

impl_generate_config_from_default!(AwsS3Config);

#[async_trait::async_trait]
#[typetag::serde(name = "aws_s3")]
impl SourceConfig for AwsS3Config {
    async fn build(&self, cx: SourceContext) -> crate::Result<super::Source> {
        let log_namespace = cx.log_namespace(self.log_namespace);

        let multiline_config: Option<line_agg::Config> = self
            .multiline
            .as_ref()
            .map(|config| config.try_into())
            .transpose()?;

        match self.strategy {
            Strategy::Sqs => Ok(Box::pin(
                self.create_sqs_ingestor(multiline_config, &cx.proxy, log_namespace)
                    .await?
                    .run(cx, self.acknowledgements, log_namespace),
            )),
        }
    }

    fn outputs(&self, global_log_namespace: LogNamespace) -> Vec<SourceOutput> {
        let log_namespace = global_log_namespace.merge(self.log_namespace);
        let mut schema_definition = self
            .decoding
            .schema_definition(log_namespace)
            .with_source_metadata(
                Self::NAME,
                Some(LegacyKey::Overwrite(owned_value_path!("bucket"))),
                &owned_value_path!("bucket"),
                Kind::bytes(),
                None,
            )
            .with_source_metadata(
                Self::NAME,
                Some(LegacyKey::Overwrite(owned_value_path!("object"))),
                &owned_value_path!("object"),
                Kind::bytes(),
                None,
            )
            .with_source_metadata(
                Self::NAME,
                Some(LegacyKey::Overwrite(owned_value_path!("region"))),
                &owned_value_path!("region"),
                Kind::bytes(),
                None,
            )
            .with_source_metadata(
                Self::NAME,
                None,
                &owned_value_path!("timestamp"),
                Kind::timestamp(),
                Some("timestamp"),
            )
            .with_standard_vector_source_metadata()
            // for metadata that is added to the events dynamically from the metadata
            .with_source_metadata(
                Self::NAME,
                None,
                &owned_value_path!("metadata"),
                Kind::object(Collection::empty().with_unknown(Kind::bytes())).or_undefined(),
                None,
            );

        // for metadata that is added to the events dynamically from the metadata
        if log_namespace == LogNamespace::Legacy {
            schema_definition = schema_definition.unknown_fields(Kind::bytes());
        }

        vec![SourceOutput::new_logs(
            self.decoding.output_type(),
            schema_definition,
        )]
    }

    fn can_acknowledge(&self) -> bool {
        true
    }
}

impl AwsS3Config {
    async fn create_sqs_ingestor(
        &self,
        multiline: Option<line_agg::Config>,
        proxy: &ProxyConfig,
        log_namespace: LogNamespace,
    ) -> crate::Result<sqs::Ingestor> {
        let region = self.region.region();
        let endpoint = self.region.endpoint();

        let s3_client = create_client::<S3ClientBuilder>(
            &self.auth,
            region.clone(),
            endpoint.clone(),
            proxy,
            &self.tls_options,
        )
        .await?;

        let decoder =
            DecodingConfig::new(self.framing.clone(), self.decoding.clone(), log_namespace)
                .build()?;

        match self.sqs {
            Some(ref sqs) => {
                let (sqs_client, region) = create_client_and_region::<SqsClientBuilder>(
                    &self.auth,
                    region.clone(),
                    endpoint,
                    proxy,
                    &sqs.tls_options,
                )
                .await?;

                let ingestor = sqs::Ingestor::new(
                    region,
                    sqs_client,
                    s3_client,
                    sqs.clone(),
                    self.compression,
                    multiline,
                    decoder,
                )
                .await?;

                Ok(ingestor)
            }
            None => Err(CreateSqsIngestorError::ConfigMissing {}.into()),
        }
    }
}

#[derive(Debug, Snafu)]
enum CreateSqsIngestorError {
    #[snafu(display("Unable to initialize: {}", source))]
    Initialize { source: sqs::IngestorNewError },
    #[snafu(display("Unable to create AWS client: {}", source))]
    Client { source: crate::Error },
    #[snafu(display("Unable to create AWS credentials provider: {}", source))]
    Credentials { source: crate::Error },
    #[snafu(display("Configuration for `sqs` required when strategy=sqs"))]
    ConfigMissing,
    #[snafu(display("Endpoint is invalid"))]
    InvalidEndpoint,
}

/// None if body is empty
async fn s3_object_decoder(
    compression: Compression,
    key: &str,
    content_encoding: Option<&str>,
    content_type: Option<&str>,
    mut body: ByteStream,
) -> Box<dyn tokio::io::AsyncRead + Send + Unpin> {
    let first = if let Some(first) = body.next().await {
        first
    } else {
        return Box::new(tokio::io::empty());
    };

    let r = tokio::io::BufReader::new(StreamReader::new(
        stream::iter(Some(first))
            .chain(Box::pin(async_stream::stream! {
                while let Some(next) = body.next().await {
                    yield next;
                }
            }))
            .map_err(|e| std::io::Error::new(ErrorKind::Other, e)),
    ));

    let compression = match compression {
        Auto => determine_compression(content_encoding, content_type, key).unwrap_or(None),
        _ => compression,
    };

    use Compression::*;
    match compression {
        Auto => unreachable!(), // is mapped above
        None => Box::new(r),
        Gzip => Box::new({
            let mut decoder = bufread::GzipDecoder::new(r);
            decoder.multiple_members(true);
            decoder
        }),
        Zstd => Box::new({
            let mut decoder = bufread::ZstdDecoder::new(r);
            decoder.multiple_members(true);
            decoder
        }),
    }
}

// try to determine the compression given the:
// * content-encoding
// * content-type
// * key name (for file extension)
//
// It will use this information in this order
fn determine_compression(
    content_encoding: Option<&str>,
    content_type: Option<&str>,
    key: &str,
) -> Option<Compression> {
    content_encoding
        .and_then(content_encoding_to_compression)
        .or_else(|| content_type.and_then(content_type_to_compression))
        .or_else(|| object_key_to_compression(key))
}

fn content_encoding_to_compression(content_encoding: &str) -> Option<Compression> {
    match content_encoding {
        "gzip" => Some(Compression::Gzip),
        "zstd" => Some(Compression::Zstd),
        _ => None,
    }
}

fn content_type_to_compression(content_type: &str) -> Option<Compression> {
    match content_type {
        "application/gzip" | "application/x-gzip" => Some(Compression::Gzip),
        "application/zstd" => Some(Compression::Zstd),
        _ => None,
    }
}

fn object_key_to_compression(key: &str) -> Option<Compression> {
    let extension = std::path::Path::new(key)
        .extension()
        .and_then(std::ffi::OsStr::to_str);

    use Compression::*;
    extension.and_then(|extension| match extension {
        "gz" => Some(Gzip),
        "zst" => Some(Zstd),
        _ => Option::None,
    })
}

#[cfg(test)]
mod test {
    use tokio::io::AsyncReadExt;

    use super::*;

    #[test]
    fn determine_compression() {
        use super::Compression;

        let cases = vec![
            ("out.log", Some("gzip"), None, Some(Compression::Gzip)),
            (
                "out.log",
                None,
                Some("application/gzip"),
                Some(Compression::Gzip),
            ),
            ("out.log.gz", None, None, Some(Compression::Gzip)),
            ("out.txt", None, None, None),
        ];
        for case in cases {
            let (key, content_encoding, content_type, expected) = case;
            assert_eq!(
                super::determine_compression(content_encoding, content_type, key),
                expected,
                "key={:?} content_encoding={:?} content_type={:?}",
                key,
                content_encoding,
                content_type,
            );
        }
    }

    #[tokio::test]
    async fn decode_empty_message_gzip() {
        let key = uuid::Uuid::new_v4().to_string();

        let mut data = Vec::new();
        s3_object_decoder(
            Compression::Auto,
            &key,
            Some("gzip"),
            None,
            ByteStream::default(),
        )
        .await
        .read_to_end(&mut data)
        .await
        .unwrap();

        assert!(data.is_empty());
    }
}

#[cfg(feature = "aws-s3-integration-tests")]
#[cfg(test)]
mod integration_tests {
    use std::{
        any::Any,
        collections::HashMap,
        fs::File,
        io::{self, BufRead},
        path::Path,
        time::Duration,
    };

    use aws_sdk_s3::Client as S3Client;
    use aws_sdk_sqs::{types::QueueAttributeName, Client as SqsClient};
    use similar_asserts::assert_eq;
    use vector_lib::codecs::{decoding::DeserializerConfig, JsonDeserializerConfig};
    use vector_lib::lookup::path;
    use vrl::value::Value;

    use super::*;
    use crate::{
        aws::{create_client, AwsAuthentication, RegionOrEndpoint},
        common::sqs::SqsClientBuilder,
        config::{ProxyConfig, SourceConfig, SourceContext},
        event::EventStatus::{self, *},
        line_agg,
        sources::{
            aws_s3::{sqs::S3Event, S3ClientBuilder},
            util::MultilineConfig,
        },
        test_util::{
            collect_n,
            components::{assert_source_compliance, SOURCE_TAGS},
            lines_from_gzip_file, random_lines, trace_init,
        },
        SourceSender,
    };

    fn lines_from_plaintext<P: AsRef<Path>>(path: P) -> Vec<String> {
        let file = io::BufReader::new(File::open(path).unwrap());
        file.lines().map(|x| x.unwrap()).collect()
    }

    #[tokio::test]
    async fn s3_process_message() {
        trace_init();

        let logs: Vec<String> = random_lines(100).take(10).collect();

        test_event(
            None,
            None,
            None,
            None,
            logs.join("\n").into_bytes(),
            logs,
            Delivered,
            false,
            DeserializerConfig::Bytes,
            None,
        )
        .await;
    }

    #[tokio::test]
    async fn s3_process_json_message() {
        trace_init();

        let logs: Vec<String> = random_lines(100).take(10).collect();

        let json_logs: Vec<String> = logs
            .iter()
            .map(|msg| {
                // convert to JSON object
                format!(r#"{{"message": "{}"}}"#, msg)
            })
            .collect();

        test_event(
            None,
            None,
            None,
            None,
            json_logs.join("\n").into_bytes(),
            logs,
            Delivered,
            false,
            DeserializerConfig::Json(JsonDeserializerConfig::default()),
            None,
        )
        .await;
    }

    #[tokio::test]
    async fn s3_process_message_with_log_namespace() {
        trace_init();

        let logs: Vec<String> = random_lines(100).take(10).collect();

        test_event(
            None,
            None,
            None,
            None,
            logs.join("\n").into_bytes(),
            logs,
            Delivered,
            true,
            DeserializerConfig::Bytes,
            None,
        )
        .await;
    }

    #[tokio::test]
    async fn s3_process_message_spaces() {
        trace_init();

        let key = "key with spaces".to_string();
        let logs: Vec<String> = random_lines(100).take(10).collect();

        test_event(
            Some(key),
            None,
            None,
            None,
            logs.join("\n").into_bytes(),
            logs,
            Delivered,
            false,
            DeserializerConfig::Bytes,
            None,
        )
        .await;
    }

    #[tokio::test]
    async fn s3_process_message_special_characters() {
        trace_init();

        let key = format!("special:{}", uuid::Uuid::new_v4());
        let logs: Vec<String> = random_lines(100).take(10).collect();

        test_event(
            Some(key),
            None,
            None,
            None,
            logs.join("\n").into_bytes(),
            logs,
            Delivered,
            false,
            DeserializerConfig::Bytes,
            None,
        )
        .await;
    }

    #[tokio::test]
    async fn s3_process_message_gzip() {
        use std::io::Read;

        trace_init();

        let logs: Vec<String> = random_lines(100).take(10).collect();

        let mut gz = flate2::read::GzEncoder::new(
            io::Cursor::new(logs.join("\n").into_bytes()),
            flate2::Compression::fast(),
        );
        let mut buffer = Vec::new();
        gz.read_to_end(&mut buffer).unwrap();

        test_event(
            None,
            Some("gzip"),
            None,
            None,
            buffer,
            logs,
            Delivered,
            false,
            DeserializerConfig::Bytes,
            None,
        )
        .await;
    }

    #[tokio::test]
    async fn s3_process_message_multipart_gzip() {
        use std::io::Read;

        trace_init();

        let logs = lines_from_gzip_file("tests/data/multipart-gzip.log.gz");

        let buffer = {
            let mut file =
                File::open("tests/data/multipart-gzip.log.gz").expect("file can be opened");
            let mut data = Vec::new();
            file.read_to_end(&mut data).expect("file can be read");
            data
        };

        test_event(
            None,
            Some("gzip"),
            None,
            None,
            buffer,
            logs,
            Delivered,
            false,
            DeserializerConfig::Bytes,
            None,
        )
        .await;
    }

    #[tokio::test]
    async fn s3_process_message_multipart_zstd() {
        use std::io::Read;

        trace_init();

        let logs = lines_from_plaintext("tests/data/multipart-zst.log");

        let buffer = {
            let mut file =
                File::open("tests/data/multipart-zst.log.zst").expect("file can be opened");
            let mut data = Vec::new();
            file.read_to_end(&mut data).expect("file can be read");
            data
        };

        test_event(
            None,
            Some("zstd"),
            None,
            None,
            buffer,
            logs,
            Delivered,
            false,
            DeserializerConfig::Bytes,
            None,
        )
        .await;
    }

    #[tokio::test]
    async fn s3_process_message_multiline() {
        trace_init();

        let logs: Vec<String> = vec!["abc", "def", "geh"]
            .into_iter()
            .map(ToOwned::to_owned)
            .collect();

        test_event(
            None,
            None,
            None,
            Some(MultilineConfig {
                start_pattern: "abc".to_owned(),
                mode: line_agg::Mode::HaltWith,
                condition_pattern: "geh".to_owned(),
                timeout_ms: Duration::from_millis(1000),
            }),
            logs.join("\n").into_bytes(),
            vec!["abc\ndef\ngeh".to_owned()],
            Delivered,
            false,
            DeserializerConfig::Bytes,
            None,
        )
        .await;
    }

    // TODO: re-enable this after figuring out why it is so flakey in CI
    //       https://github.com/vectordotdev/vector/issues/17456
    #[ignore]
    #[tokio::test]
    async fn handles_errored_status() {
        trace_init();

        let logs: Vec<String> = random_lines(100).take(10).collect();

        test_event(
            None,
            None,
            None,
            None,
            logs.join("\n").into_bytes(),
            logs,
            Errored,
            false,
            DeserializerConfig::Bytes,
            None,
        )
        .await;
    }

    #[tokio::test]
    async fn handles_failed_status() {
        trace_init();

        let logs: Vec<String> = random_lines(100).take(10).collect();

        test_event(
            None,
            None,
            None,
            None,
            logs.join("\n").into_bytes(),
            logs,
            Rejected,
            false,
            DeserializerConfig::Bytes,
            None,
        )
        .await;
    }

    #[tokio::test]
    async fn handles_failed_status_without_deletion() {
        trace_init();

        let logs: Vec<String> = random_lines(100).take(10).collect();

        let mut custom_options: HashMap<String, Box<dyn Any>> = HashMap::new();
        custom_options.insert("delete_failed_message".to_string(), Box::new(false));

        test_event(
            None,
            None,
            None,
            None,
            logs.join("\n").into_bytes(),
            logs,
            Rejected,
            false,
            DeserializerConfig::Bytes,
            Some(custom_options),
        )
        .await;
    }

    fn s3_address() -> String {
        std::env::var("S3_ADDRESS").unwrap_or_else(|_| "http://localhost:4566".into())
    }

    fn config(
        queue_url: &str,
        multiline: Option<MultilineConfig>,
        log_namespace: bool,
        decoding: DeserializerConfig,
    ) -> AwsS3Config {
        AwsS3Config {
            region: RegionOrEndpoint::with_both("us-east-1", s3_address()),
            strategy: Strategy::Sqs,
            compression: Compression::Auto,
            multiline,
            sqs: Some(sqs::Config {
                queue_url: queue_url.to_string(),
                poll_secs: 1,
                visibility_timeout_secs: 0,
                client_concurrency: None,
                ..Default::default()
            }),
            acknowledgements: true.into(),
            log_namespace: Some(log_namespace),
            decoding,
            ..Default::default()
        }
    }

    // puts an object and asserts that the logs it gets back match
    #[allow(clippy::too_many_arguments)]
    async fn test_event(
        key: Option<String>,
        content_encoding: Option<&str>,
        content_type: Option<&str>,
        multiline: Option<MultilineConfig>,
        payload: Vec<u8>,
        expected_lines: Vec<String>,
        status: EventStatus,
        log_namespace: bool,
        decoding: DeserializerConfig,
        custom_options: Option<HashMap<String, Box<dyn Any>>>,
    ) {
        assert_source_compliance(&SOURCE_TAGS, async move {
            let key = key.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

            let s3 = s3_client().await;
            let sqs = sqs_client().await;

            let queue = create_queue(&sqs).await;
            let bucket = create_bucket(&s3).await;

            tokio::time::sleep(Duration::from_secs(1)).await;

            let mut config = config(&queue, multiline, log_namespace, decoding);

            if let Some(false) = custom_options
                .as_ref()
                .and_then(|opts| opts.get("delete_failed_message"))
                .and_then(|val| val.downcast_ref::<bool>())
                .copied()
            {
                config.sqs.as_mut().unwrap().delete_failed_message = false;
            }

            s3.put_object()
                .bucket(bucket.clone())
                .key(key.clone())
                .body(ByteStream::from(payload))
                .set_content_type(content_type.map(|t| t.to_owned()))
                .set_content_encoding(content_encoding.map(|t| t.to_owned()))
                .send()
                .await
                .expect("Could not put object");

            let sqs_client = sqs_client().await;

            let mut s3_event: S3Event = serde_json::from_str(
            r#"
{
   "Records":[
      {
         "eventVersion":"2.1",
         "eventSource":"aws:s3",
         "awsRegion":"us-east-1",
         "eventTime":"2022-03-24T19:43:00.548Z",
         "eventName":"ObjectCreated:Put",
         "userIdentity":{
            "principalId":"AWS:ARNOTAREALIDD4:user.name"
         },
         "requestParameters":{
            "sourceIPAddress":"136.56.73.213"
         },
         "responseElements":{
            "x-amz-request-id":"ZX6X98Q6NM9NQTP3",
            "x-amz-id-2":"ESLLtyT4N5cAPW+C9EXwtaeEWz6nq7eCA6txjZKlG2Q7xp2nHXQI69Od2B0PiYIbhUiX26NrpIQPV0lLI6js3nVNmYo2SWBs"
         },
         "s3":{
            "s3SchemaVersion":"1.0",
            "configurationId":"asdfasdf",
            "bucket":{
               "name":"bucket-name",
               "ownerIdentity":{
                  "principalId":"A3PEG170DF9VNQ"
               },
               "arn":"arn:aws:s3:::nfox-testing-vector"
            },
            "object":{
               "key":"test-log.txt",
               "size":33,
               "eTag":"c981ce6672c4251048b0b834e334007f",
               "sequencer":"00623CC9C47AB5634C"
            }
         }
      }
   ]
}
        "#,
            )
            .unwrap();

            s3_event.records[0].s3.bucket.name = bucket.clone();
            s3_event.records[0].s3.object.key = key.clone();

            // send SQS message (this is usually sent by S3 itself when an object is uploaded)
            // This does not automatically work with localstack and the AWS SDK, so this is done manually
            let _send_message_output = sqs_client
                .send_message()
                .queue_url(queue.clone())
                .message_body(serde_json::to_string(&s3_event).unwrap())
                .send()
                .await
                .unwrap();

            let (tx, rx) = SourceSender::new_test_finalize(status);
            let cx = SourceContext::new_test(tx, None);
            let namespace = cx.log_namespace(Some(log_namespace));
            let source = config.build(cx).await.unwrap();
            tokio::spawn(async move { source.await.unwrap() });

            let events = collect_n(rx, expected_lines.len()).await;

            assert_eq!(expected_lines.len(), events.len());
            for (i, event) in events.iter().enumerate() {

                if let Some(schema_definition) = config.outputs(namespace).pop().unwrap().schema_definition {
                    schema_definition.is_valid_for_event(event).unwrap();
                }

                let message = expected_lines[i].as_str();

                let log = event.as_log();
                if log_namespace {
                    assert_eq!(log.value(), &Value::from(message));
                } else {
                    assert_eq!(log["message"], message.into());
                }
                assert_eq!(namespace.get_source_metadata(AwsS3Config::NAME, log, path!("bucket"), path!("bucket")).unwrap(), &bucket.clone().into());
                assert_eq!(namespace.get_source_metadata(AwsS3Config::NAME, log, path!("object"), path!("object")).unwrap(), &key.clone().into());
                assert_eq!(namespace.get_source_metadata(AwsS3Config::NAME, log, path!("region"), path!("region")).unwrap(), &"us-east-1".into());
            }

            // Unfortunately we need a fairly large sleep here to ensure that the source has actually managed to delete the SQS message.
            // The deletion of this message occurs after the Event has been sent out by the source and there is no way of knowing when this
            // process has finished other than waiting around for a while.
            tokio::time::sleep(Duration::from_secs(10)).await;
            // Make sure the SQS message is deleted
            match status {
                Errored => {
                    // need to wait up to the visibility timeout before it will be counted again
                    assert_eq!(count_messages(&sqs, &queue, 10).await, 1);
                }
                Rejected if !config.sqs.unwrap().delete_failed_message => {
                    assert_eq!(count_messages(&sqs, &queue, 10).await, 1);
                }
                _ => {
                    assert_eq!(count_messages(&sqs, &queue, 0).await, 0);
                }
            };
        }).await;
    }

    /// creates a new SQS queue
    ///
    /// returns the queue name
    async fn create_queue(client: &SqsClient) -> String {
        let queue_name = uuid::Uuid::new_v4().to_string();

        let res = client
            .create_queue()
            .queue_name(queue_name.clone())
            .attributes(QueueAttributeName::VisibilityTimeout, "2")
            .send()
            .await
            .expect("Could not create queue");

        res.queue_url.expect("no queue url")
    }

    /// count the number of messages in a SQS queue
    async fn count_messages(client: &SqsClient, queue: &str, wait_time_seconds: i32) -> usize {
        let sqs_result = client
            .receive_message()
            .queue_url(queue)
            .visibility_timeout(0)
            .wait_time_seconds(wait_time_seconds)
            .send()
            .await
            .unwrap();

        sqs_result
            .messages
            .map(|messages| messages.len())
            .unwrap_or(0)
    }

    /// creates a new S3 bucket
    ///
    /// returns the bucket name
    async fn create_bucket(client: &S3Client) -> String {
        let bucket_name = uuid::Uuid::new_v4().to_string();

        client
            .create_bucket()
            .bucket(bucket_name.clone())
            .send()
            .await
            .expect("Could not create bucket");

        bucket_name
    }

    async fn s3_client() -> S3Client {
        let auth = AwsAuthentication::test_auth();
        let region_endpoint = RegionOrEndpoint {
            region: Some("us-east-1".to_owned()),
            endpoint: Some(s3_address()),
        };
        let proxy_config = ProxyConfig::default();
        create_client::<S3ClientBuilder>(
            &auth,
            region_endpoint.region(),
            region_endpoint.endpoint(),
            &proxy_config,
            &None,
        )
        .await
        .unwrap()
    }

    async fn sqs_client() -> SqsClient {
        let auth = AwsAuthentication::test_auth();
        let region_endpoint = RegionOrEndpoint {
            region: Some("us-east-1".to_owned()),
            endpoint: Some(s3_address()),
        };
        let proxy_config = ProxyConfig::default();
        create_client::<SqsClientBuilder>(
            &auth,
            region_endpoint.region(),
            region_endpoint.endpoint(),
            &proxy_config,
            &None,
        )
        .await
        .unwrap()
    }
}
