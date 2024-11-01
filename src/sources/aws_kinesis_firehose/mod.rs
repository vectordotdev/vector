use std::time::Duration;
use std::{convert::Infallible, fmt, net::SocketAddr};

use futures::FutureExt;
use hyper::{service::make_service_fn, Server};
use tokio::net::TcpStream;
use tower::ServiceBuilder;
use tracing::Span;
use vector_lib::codecs::decoding::{DeserializerConfig, FramingConfig};
use vector_lib::config::{LegacyKey, LogNamespace};
use vector_lib::configurable::configurable_component;
use vector_lib::lookup::owned_value_path;
use vector_lib::sensitive_string::SensitiveString;
use vector_lib::tls::MaybeTlsIncomingStream;
use vrl::value::Kind;

use crate::http::{KeepaliveConfig, MaxConnectionAgeLayer};
use crate::{
    codecs::DecodingConfig,
    config::{
        GenerateConfig, Resource, SourceAcknowledgementsConfig, SourceConfig, SourceContext,
        SourceOutput,
    },
    http::build_http_trace_layer,
    serde::{bool_or_struct, default_decoding, default_framing_message_based},
    tls::{MaybeTlsSettings, TlsEnableableConfig},
};

pub mod errors;
mod filters;
mod handlers;
mod models;

/// Configuration for the `aws_kinesis_firehose` source.
#[configurable_component(source(
    "aws_kinesis_firehose",
    "Collect logs from AWS Kinesis Firehose."
))]
#[derive(Clone, Debug)]
pub struct AwsKinesisFirehoseConfig {
    /// The socket address to listen for connections on.
    #[configurable(metadata(docs::examples = "0.0.0.0:443"))]
    #[configurable(metadata(docs::examples = "localhost:443"))]
    address: SocketAddr,

    /// An access key to authenticate requests against.
    ///
    /// AWS Kinesis Firehose can be configured to pass along a user-configurable access key with each request. If
    /// configured, `access_key` should be set to the same value. Otherwise, all requests are allowed.
    #[configurable(deprecated = "This option has been deprecated, use `access_keys` instead.")]
    #[configurable(metadata(docs::examples = "A94A8FE5CCB19BA61C4C08"))]
    access_key: Option<SensitiveString>,

    /// A list of access keys to authenticate requests against.
    ///
    /// AWS Kinesis Firehose can be configured to pass along a user-configurable access key with each request. If
    /// configured, `access_keys` should be set to the same value. Otherwise, all requests are allowed.
    #[configurable(metadata(docs::examples = "access_keys_example()"))]
    access_keys: Option<Vec<SensitiveString>>,

    /// Whether or not to store the AWS Firehose Access Key in event secrets.
    ///
    /// If set to `true`, when incoming requests contains an access key sent by AWS Firehose, it is kept in the
    /// event secrets as "aws_kinesis_firehose_access_key".
    #[configurable(derived)]
    store_access_key: bool,

    /// The compression scheme to use for decompressing records within the Firehose message.
    ///
    /// Some services, like AWS CloudWatch Logs, [compresses the events with gzip][events_with_gzip],
    /// before sending them AWS Kinesis Firehose. This option can be used to automatically decompress
    /// them before forwarding them to the next component.
    ///
    /// Note that this is different from [Content encoding option][encoding_option] of the
    /// Firehose HTTP endpoint destination. That option controls the content encoding of the entire HTTP request.
    ///
    /// [events_with_gzip]: https://docs.aws.amazon.com/firehose/latest/dev/writing-with-cloudwatch-logs.html
    /// [encoding_option]: https://docs.aws.amazon.com/firehose/latest/dev/create-destination.html#create-destination-http
    #[serde(default)]
    record_compression: Compression,

    #[configurable(derived)]
    tls: Option<TlsEnableableConfig>,

    #[configurable(derived)]
    #[configurable(metadata(docs::advanced))]
    #[serde(default = "default_framing_message_based")]
    framing: FramingConfig,

    #[configurable(derived)]
    #[configurable(metadata(docs::advanced))]
    #[serde(default = "default_decoding")]
    decoding: DeserializerConfig,

    #[configurable(derived)]
    #[serde(default, deserialize_with = "bool_or_struct")]
    acknowledgements: SourceAcknowledgementsConfig,

    /// The namespace to use for logs. This overrides the global setting.
    #[configurable(metadata(docs::hidden))]
    #[serde(default)]
    log_namespace: Option<bool>,

    #[configurable(derived)]
    #[serde(default)]
    keepalive: KeepaliveConfig,
}

const fn access_keys_example() -> [&'static str; 2] {
    ["A94A8FE5CCB19BA61C4C08", "B94B8FE5CCB19BA61C4C12"]
}

/// Compression scheme for records in a Firehose message.
#[configurable_component]
#[configurable(metadata(docs::advanced))]
#[derive(Clone, Copy, Debug, Derivative, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
#[derivative(Default)]
pub enum Compression {
    /// Automatically attempt to determine the compression scheme.
    ///
    /// The compression scheme of the object is determined by looking at its file signature, also known
    /// as [magic bytes][magic_bytes].
    ///
    /// If the record fails to decompress with the discovered format, the record is forwarded as is.
    /// Thus, if you know the records are always gzip encoded (for example, if they are coming from AWS CloudWatch Logs),
    /// set `gzip` in this field so that any records that are not-gzipped are rejected.
    ///
    /// [magic_bytes]: https://en.wikipedia.org/wiki/List_of_file_signatures
    #[derivative(Default)]
    Auto,

    /// Uncompressed.
    None,

    /// GZIP.
    Gzip,
}

impl fmt::Display for Compression {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        match self {
            Compression::Auto => write!(fmt, "auto"),
            Compression::None => write!(fmt, "none"),
            Compression::Gzip => write!(fmt, "gzip"),
        }
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "aws_kinesis_firehose")]
impl SourceConfig for AwsKinesisFirehoseConfig {
    async fn build(&self, cx: SourceContext) -> crate::Result<super::Source> {
        let log_namespace = cx.log_namespace(self.log_namespace);
        let decoder =
            DecodingConfig::new(self.framing.clone(), self.decoding.clone(), log_namespace)
                .build()?;

        let acknowledgements = cx.do_acknowledgements(self.acknowledgements);

        if self.access_key.is_some() {
            warn!("DEPRECATION `access_key`, use `access_keys` instead.")
        }

        // Merge with legacy `access_key`
        let access_keys = self
            .access_keys
            .iter()
            .flatten()
            .chain(self.access_key.iter());

        let svc = filters::firehose(
            access_keys.map(|key| key.inner().to_string()).collect(),
            self.store_access_key,
            self.record_compression,
            decoder,
            acknowledgements,
            cx.out,
            log_namespace,
        );

        let tls = MaybeTlsSettings::from_config(&self.tls, true)?;
        let listener = tls.bind(&self.address).await?;

        let keepalive_settings = self.keepalive.clone();
        let shutdown = cx.shutdown;
        Ok(Box::pin(async move {
            let span = Span::current();
            let make_svc = make_service_fn(move |conn: &MaybeTlsIncomingStream<TcpStream>| {
                let svc = ServiceBuilder::new()
                    .layer(build_http_trace_layer(span.clone()))
                    .option_layer(keepalive_settings.max_connection_age_secs.map(|secs| {
                        MaxConnectionAgeLayer::new(
                            Duration::from_secs(secs),
                            keepalive_settings.max_connection_age_jitter_factor,
                            conn.peer_addr(),
                        )
                    }))
                    .service(warp::service(svc.clone()));
                futures_util::future::ok::<_, Infallible>(svc)
            });

            Server::builder(hyper::server::accept::from_stream(listener.accept_stream()))
                .serve(make_svc)
                .with_graceful_shutdown(shutdown.map(|_| ()))
                .await
                .map_err(|err| {
                    error!("An error occurred: {:?}.", err);
                })?;

            Ok(())
        }))
    }

    fn outputs(&self, global_log_namespace: LogNamespace) -> Vec<SourceOutput> {
        let schema_definition = self
            .decoding
            .schema_definition(global_log_namespace.merge(self.log_namespace))
            .with_standard_vector_source_metadata()
            .with_source_metadata(
                Self::NAME,
                Some(LegacyKey::InsertIfEmpty(owned_value_path!("request_id"))),
                &owned_value_path!("request_id"),
                Kind::bytes(),
                None,
            )
            .with_source_metadata(
                Self::NAME,
                Some(LegacyKey::InsertIfEmpty(owned_value_path!("source_arn"))),
                &owned_value_path!("source_arn"),
                Kind::bytes(),
                None,
            );

        vec![SourceOutput::new_maybe_logs(
            self.decoding.output_type(),
            schema_definition,
        )]
    }

    fn resources(&self) -> Vec<Resource> {
        vec![Resource::tcp(self.address)]
    }

    fn can_acknowledge(&self) -> bool {
        true
    }
}

impl GenerateConfig for AwsKinesisFirehoseConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(Self {
            address: "0.0.0.0:443".parse().unwrap(),
            access_key: None,
            access_keys: None,
            store_access_key: false,
            tls: None,
            record_compression: Default::default(),
            framing: default_framing_message_based(),
            decoding: default_decoding(),
            acknowledgements: Default::default(),
            log_namespace: None,
            keepalive: Default::default(),
        })
        .unwrap()
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::print_stdout)] //tests

    use std::{
        io::{Cursor, Read},
        net::SocketAddr,
    };

    use base64::prelude::{Engine as _, BASE64_STANDARD};
    use bytes::Bytes;
    use chrono::{DateTime, SubsecRound, Utc};
    use flate2::read::GzEncoder;
    use futures::Stream;
    use similar_asserts::assert_eq;
    use tokio::time::{sleep, Duration};
    use vector_lib::assert_event_data_eq;
    use vector_lib::lookup::path;
    use vrl::value;

    use super::*;
    use crate::{
        event::{Event, EventStatus},
        log_event,
        test_util::{
            collect_ready,
            components::{assert_source_compliance, SOURCE_TAGS},
            next_addr, wait_for_tcp,
        },
        SourceSender,
    };

    const SOURCE_ARN: &str = "arn:aws:firehose:us-east-1:111111111111:deliverystream/test";
    const REQUEST_ID: &str = "e17265d6-97af-4938-982e-90d5614c4242";
    // example CloudWatch Logs subscription event
    const RECORD: &str = r#"
            {
                "messageType": "DATA_MESSAGE",
                "owner": "071959437513",
                "logGroup": "/jesse/test",
                "logStream": "test",
                "subscriptionFilters": ["Destination"],
                "logEvents": [
                    {
                        "id": "35683658089614582423604394983260738922885519999578275840",
                        "timestamp": 1600110569039,
                        "message": "{\"bytes\":26780,\"datetime\":\"14/Sep/2020:11:45:41 -0400\",\"host\":\"157.130.216.193\",\"method\":\"PUT\",\"protocol\":\"HTTP/1.0\",\"referer\":\"https://www.principalcross-platform.io/markets/ubiquitous\",\"request\":\"/expedite/convergence\",\"source_type\":\"stdin\",\"status\":301,\"user-identifier\":\"-\"}"
                    },
                    {
                        "id": "35683658089659183914001456229543810359430816722590236673",
                        "timestamp": 1600110569041,
                        "message": "{\"bytes\":17707,\"datetime\":\"14/Sep/2020:11:45:41 -0400\",\"host\":\"109.81.244.252\",\"method\":\"GET\",\"protocol\":\"HTTP/2.0\",\"referer\":\"http://www.investormission-critical.io/24/7/vortals\",\"request\":\"/scale/functionalities/optimize\",\"source_type\":\"stdin\",\"status\":502,\"user-identifier\":\"feeney1708\"}"
                    }
                ]
            }
        "#;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<AwsKinesisFirehoseConfig>();
    }

    async fn source(
        access_key: Option<SensitiveString>,
        access_keys: Option<Vec<SensitiveString>>,
        store_access_key: bool,
        record_compression: Compression,
        delivered: bool,
        log_namespace: bool,
    ) -> (impl Stream<Item = Event> + Unpin, SocketAddr) {
        use EventStatus::*;
        let status = if delivered { Delivered } else { Rejected };
        let (sender, recv) = SourceSender::new_test_finalize(status);
        let address = next_addr();
        let cx = SourceContext::new_test(sender, None);
        tokio::spawn(async move {
            AwsKinesisFirehoseConfig {
                address,
                tls: None,
                access_key,
                access_keys,
                store_access_key,
                record_compression,
                framing: default_framing_message_based(),
                decoding: default_decoding(),
                acknowledgements: true.into(),
                log_namespace: Some(log_namespace),
                keepalive: Default::default(),
            }
            .build(cx)
            .await
            .unwrap()
            .await
            .unwrap()
        });
        wait_for_tcp(address).await;
        (recv, address)
    }

    /// Sends the body to the address with the appropriate Firehose headers
    ///
    /// https://docs.aws.amazon.com/firehose/latest/dev/httpdeliveryrequestresponse.html
    async fn send(
        address: SocketAddr,
        timestamp: DateTime<Utc>,
        records: Vec<&[u8]>,
        key: Option<&str>,
        gzip: bool,
        record_compression: Compression,
    ) -> reqwest::Result<reqwest::Response> {
        let request = models::FirehoseRequest {
            access_key: key.map(|s| s.to_string()),
            request_id: REQUEST_ID.to_string(),
            timestamp,
            records: records
                .into_iter()
                .map(|record| models::EncodedFirehoseRecord {
                    data: encode_record(record, record_compression).unwrap(),
                })
                .collect(),
        };

        let mut builder = reqwest::Client::new()
            .post(format!("http://{}", address))
            .header("host", address.to_string())
            .header(
                "x-amzn-trace-id",
                "Root=1-5f5fbf1c-877c68cace58bea222ddbeec",
            )
            .header("x-amz-firehose-protocol-version", "1.0")
            .header("x-amz-firehose-request-id", REQUEST_ID.to_string())
            .header("x-amz-firehose-source-arn", SOURCE_ARN.to_string())
            .header("user-agent", "Amazon Kinesis Data Firehose Agent/1.0")
            .header("content-type", "application/json");

        if let Some(key) = key {
            builder = builder.header("x-amz-firehose-access-key", key);
        }

        if gzip {
            let mut gz = GzEncoder::new(
                Cursor::new(serde_json::to_vec(&request).unwrap()),
                flate2::Compression::fast(),
            );
            let mut buffer = Vec::new();
            gz.read_to_end(&mut buffer).unwrap();
            builder = builder.header("content-encoding", "gzip").body(buffer);
        } else {
            builder = builder.json(&request);
        }

        builder.send().await
    }

    async fn spawn_send(
        address: SocketAddr,
        timestamp: DateTime<Utc>,
        records: Vec<&'static [u8]>,
        key: Option<&'static str>,
        gzip: bool,
        record_compression: Compression,
    ) -> tokio::task::JoinHandle<reqwest::Result<reqwest::Response>> {
        let handle = tokio::spawn(async move {
            send(address, timestamp, records, key, gzip, record_compression).await
        });
        sleep(Duration::from_millis(100)).await;
        handle
    }

    /// Encodes record data to mach AWS's representation: base64 encoded with an additional
    /// compression
    fn encode_record(record: &[u8], compression: Compression) -> std::io::Result<String> {
        let compressed = match compression {
            Compression::Auto => panic!("cannot encode records as Auto"),
            Compression::Gzip => {
                let mut buffer = Vec::new();
                if !record.is_empty() {
                    let mut gz = GzEncoder::new(record, flate2::Compression::fast());
                    gz.read_to_end(&mut buffer)?;
                }
                buffer
            }
            Compression::None => record.to_vec(),
        };

        Ok(BASE64_STANDARD.encode(compressed))
    }

    #[tokio::test]
    async fn aws_kinesis_firehose_forwards_events_legacy_namespace() {
        let gzipped_record = {
            let mut buf = Vec::new();
            let mut gz = GzEncoder::new(RECORD.as_bytes(), flate2::Compression::fast());
            gz.read_to_end(&mut buf).unwrap();
            buf
        };

        for (source_record_compression, record_compression, success, record, expected) in [
            (
                Compression::Auto,
                Compression::Gzip,
                true,
                RECORD.as_bytes(),
                RECORD.as_bytes().to_owned(),
            ),
            (
                Compression::Auto,
                Compression::None,
                true,
                RECORD.as_bytes(),
                RECORD.as_bytes().to_owned(),
            ),
            (
                Compression::None,
                Compression::Gzip,
                true,
                RECORD.as_bytes(),
                gzipped_record,
            ),
            (
                Compression::None,
                Compression::None,
                true,
                RECORD.as_bytes(),
                RECORD.as_bytes().to_owned(),
            ),
            (
                Compression::Gzip,
                Compression::Gzip,
                true,
                RECORD.as_bytes(),
                RECORD.as_bytes().to_owned(),
            ),
            (
                Compression::Gzip,
                Compression::None,
                false,
                RECORD.as_bytes(),
                RECORD.as_bytes().to_owned(),
            ),
            (
                Compression::Gzip,
                Compression::Gzip,
                true,
                "".as_bytes(),
                Vec::new(),
            ),
        ] {
            let (rx, addr) =
                source(None, None, false, source_record_compression, true, false).await;

            let timestamp: DateTime<Utc> = Utc::now();

            let res = spawn_send(
                addr,
                timestamp,
                vec![record],
                None,
                false,
                record_compression,
            )
            .await;

            if success {
                let events = collect_ready(rx).await;

                let res = res.await.unwrap().unwrap();
                assert_eq!(200, res.status().as_u16());

                assert_event_data_eq!(
                    events,
                    vec![log_event! {
                        "source_type" => Bytes::from("aws_kinesis_firehose"),
                        "timestamp" => timestamp.trunc_subsecs(3), // AWS sends timestamps as ms
                        "message" => Bytes::from(expected),
                        "request_id" => REQUEST_ID,
                        "source_arn" => SOURCE_ARN,
                    },]
                );

                let response: models::FirehoseResponse = res.json().await.unwrap();
                assert_eq!(response.request_id, REQUEST_ID);
            } else {
                let res = res.await.unwrap().unwrap();
                assert_eq!(400, res.status().as_u16());
            }
        }
    }

    #[tokio::test]
    async fn aws_kinesis_firehose_forwards_events_vector_namespace() {
        let gzipped_record = {
            let mut buf = Vec::new();
            let mut gz = GzEncoder::new(RECORD.as_bytes(), flate2::Compression::fast());
            gz.read_to_end(&mut buf).unwrap();
            buf
        };

        for (source_record_compression, record_compression, success, record, expected) in [
            (
                Compression::Auto,
                Compression::Gzip,
                true,
                RECORD.as_bytes(),
                RECORD.as_bytes().to_owned(),
            ),
            (
                Compression::Auto,
                Compression::None,
                true,
                RECORD.as_bytes(),
                RECORD.as_bytes().to_owned(),
            ),
            (
                Compression::None,
                Compression::Gzip,
                true,
                RECORD.as_bytes(),
                gzipped_record,
            ),
            (
                Compression::None,
                Compression::None,
                true,
                RECORD.as_bytes(),
                RECORD.as_bytes().to_owned(),
            ),
            (
                Compression::Gzip,
                Compression::Gzip,
                true,
                RECORD.as_bytes(),
                RECORD.as_bytes().to_owned(),
            ),
            (
                Compression::Gzip,
                Compression::None,
                false,
                RECORD.as_bytes(),
                RECORD.as_bytes().to_owned(),
            ),
            (
                Compression::Gzip,
                Compression::Gzip,
                true,
                "".as_bytes(),
                Vec::new(),
            ),
        ] {
            let (rx, addr) = source(None, None, false, source_record_compression, true, true).await;

            let timestamp: DateTime<Utc> = Utc::now();

            let res = spawn_send(
                addr,
                timestamp,
                vec![record],
                None,
                false,
                record_compression,
            )
            .await;

            if success {
                let events = collect_ready(rx).await;

                let res = res.await.unwrap().unwrap();
                assert_eq!(200, res.status().as_u16());

                for event in events {
                    let log = event.as_log();
                    let meta = log.metadata();

                    // event data, currently assumes default bytes deserializer
                    assert_eq!(log.value(), &value!(Bytes::from(expected.to_owned())));

                    // vector metadata
                    assert_eq!(
                        meta.value().get(path!("vector", "source_type")).unwrap(),
                        &value!("aws_kinesis_firehose")
                    );
                    assert!(meta
                        .value()
                        .get(path!("vector", "ingest_timestamp"))
                        .unwrap()
                        .is_timestamp());

                    // source metadata
                    assert_eq!(
                        meta.value()
                            .get(path!("aws_kinesis_firehose", "request_id"))
                            .unwrap(),
                        &value!(REQUEST_ID)
                    );
                    assert_eq!(
                        meta.value()
                            .get(path!("aws_kinesis_firehose", "source_arn"))
                            .unwrap(),
                        &value!(SOURCE_ARN)
                    );
                    assert_eq!(
                        meta.value()
                            .get(path!("aws_kinesis_firehose", "timestamp"))
                            .unwrap(),
                        &value!(timestamp.trunc_subsecs(3))
                    );
                }

                let response: models::FirehoseResponse = res.json().await.unwrap();
                assert_eq!(response.request_id, REQUEST_ID);
            } else {
                let res = res.await.unwrap().unwrap();
                assert_eq!(400, res.status().as_u16());
            }
        }
    }

    #[tokio::test]
    async fn aws_kinesis_firehose_forwards_events_gzip_request() {
        assert_source_compliance(&SOURCE_TAGS, async move {
            let (rx, addr) = source(None, None, false, Default::default(), true, false).await;

            let timestamp: DateTime<Utc> = Utc::now();

            let res = spawn_send(
                addr,
                timestamp,
                vec![RECORD.as_bytes()],
                None,
                true,
                Compression::None,
            )
            .await;

            let events = collect_ready(rx).await;
            let res = res.await.unwrap().unwrap();
            assert_eq!(200, res.status().as_u16());

            assert_event_data_eq!(
                events,
                vec![log_event! {
                    "source_type" => Bytes::from("aws_kinesis_firehose"),
                    "timestamp" => timestamp.trunc_subsecs(3), // AWS sends timestamps as ms
                    "message"=> RECORD,
                    "request_id" => REQUEST_ID,
                    "source_arn" => SOURCE_ARN,
                },]
            );

            let response: models::FirehoseResponse = res.json().await.unwrap();
            assert_eq!(response.request_id, REQUEST_ID);
        })
        .await;
    }

    #[tokio::test]
    async fn aws_kinesis_firehose_rejects_bad_access_key() {
        let (_rx, addr) = source(
            Some("an access key".to_string().into()),
            Some(vec!["an access key in list".to_string().into()]),
            Default::default(),
            Default::default(),
            true,
            false,
        )
        .await;

        let res = send(
            addr,
            Utc::now(),
            vec![],
            Some("bad access key"),
            false,
            Compression::None,
        )
        .await
        .unwrap();
        assert_eq!(401, res.status().as_u16());

        let response: models::FirehoseResponse = res.json().await.unwrap();
        assert_eq!(response.request_id, REQUEST_ID);
    }

    #[tokio::test]
    async fn aws_kinesis_firehose_rejects_bad_access_key_from_list() {
        let (_rx, addr) = source(
            None,
            Some(vec!["an access key in list".to_string().into()]),
            Default::default(),
            Default::default(),
            true,
            false,
        )
        .await;

        let res = send(
            addr,
            Utc::now(),
            vec![],
            Some("bad access key"),
            false,
            Compression::None,
        )
        .await
        .unwrap();
        assert_eq!(401, res.status().as_u16());

        let response: models::FirehoseResponse = res.json().await.unwrap();
        assert_eq!(response.request_id, REQUEST_ID);
    }

    #[tokio::test]
    async fn aws_kinesis_firehose_accepts_merged_access_keys() {
        let valid_access_key = SensitiveString::from(String::from("an access key in list"));

        let (_rx, addr) = source(
            Some(valid_access_key.clone()),
            Some(vec!["valid access key 2".to_string().into()]),
            Default::default(),
            Default::default(),
            true,
            false,
        )
        .await;

        let res = send(
            addr,
            Utc::now(),
            vec![],
            Some(valid_access_key.clone().inner()),
            false,
            Compression::None,
        )
        .await
        .unwrap();

        assert_eq!(200, res.status().as_u16());

        let response: models::FirehoseResponse = res.json().await.unwrap();
        assert_eq!(response.request_id, REQUEST_ID);
    }

    #[tokio::test]
    async fn aws_kinesis_firehose_accepts_access_keys_from_list() {
        let valid_access_key = "an access key in list".to_string();

        let (_rx, addr) = source(
            None,
            Some(vec![
                valid_access_key.clone().into(),
                "valid access key 2".to_string().into(),
            ]),
            Default::default(),
            Default::default(),
            true,
            false,
        )
        .await;

        let res = send(
            addr,
            Utc::now(),
            vec![],
            Some(&valid_access_key),
            false,
            Compression::None,
        )
        .await
        .unwrap();

        assert_eq!(200, res.status().as_u16());

        let response: models::FirehoseResponse = res.json().await.unwrap();
        assert_eq!(response.request_id, REQUEST_ID);
    }

    #[tokio::test]
    async fn handles_acknowledgement_failure() {
        let expected = RECORD.as_bytes().to_owned();

        let (rx, addr) = source(None, None, false, Compression::None, false, false).await;

        let timestamp: DateTime<Utc> = Utc::now();

        let res = spawn_send(
            addr,
            timestamp,
            vec![RECORD.as_bytes()],
            None,
            false,
            Compression::None,
        )
        .await;

        let events = collect_ready(rx).await;

        let res = res.await.unwrap().unwrap();
        assert_eq!(406, res.status().as_u16());

        assert_event_data_eq!(
            events,
            vec![log_event! {
                "source_type" => Bytes::from("aws_kinesis_firehose"),
                "timestamp" => timestamp.trunc_subsecs(3), // AWS sends timestamps as ms
                "message"=> Bytes::from(expected),
                "request_id" => REQUEST_ID,
                "source_arn" => SOURCE_ARN,
            },]
        );

        let response: models::FirehoseResponse = res.json().await.unwrap();
        assert_eq!(response.request_id, REQUEST_ID);
    }

    #[tokio::test]
    async fn event_access_key_passthrough_enabled() {
        let (rx, address) = source(
            None,
            Some(vec!["an access key".to_string().into()]),
            true,
            Default::default(),
            true,
            true,
        )
        .await;

        let timestamp: DateTime<Utc> = Utc::now();

        spawn_send(
            address,
            timestamp,
            vec![RECORD.as_bytes()],
            Some("an access key"),
            false,
            Compression::None,
        )
        .await;

        let events = collect_ready(rx).await;
        let access_key = events[0]
            .metadata()
            .secrets()
            .get("aws_kinesis_firehose_access_key")
            .unwrap();
        assert_eq!(access_key.to_string(), "an access key".to_string());
    }

    #[tokio::test]
    async fn no_authorization_access_key_passthrough_enabled() {
        let (rx, address) = source(None, None, true, Default::default(), true, true).await;

        let timestamp: DateTime<Utc> = Utc::now();

        spawn_send(
            address,
            timestamp,
            vec![RECORD.as_bytes()],
            None,
            false,
            Compression::None,
        )
        .await;

        let events = collect_ready(rx).await;

        assert!(events[0]
            .metadata()
            .secrets()
            .get("aws_kinesis_firehose_access_key")
            .is_none());
    }
}
