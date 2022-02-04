use std::{fmt, net::SocketAddr};

use futures::FutureExt;
use serde::{Deserialize, Serialize};
use warp::Filter;

use crate::{
    codecs::decoding::{DecodingConfig, DeserializerConfig, FramingConfig},
    config::{
        AcknowledgementsConfig, DataType, GenerateConfig, Output, Resource, SourceConfig,
        SourceContext, SourceDescription,
    },
    serde::{bool_or_struct, default_decoding, default_framing_message_based},
    tls::{MaybeTlsSettings, TlsConfig},
};

pub mod errors;
mod filters;
mod handlers;
mod models;

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct AwsKinesisFirehoseConfig {
    address: SocketAddr,
    access_key: Option<String>,
    tls: Option<TlsConfig>,
    record_compression: Option<Compression>,
    #[serde(default = "default_framing_message_based")]
    framing: Box<dyn FramingConfig>,
    #[serde(default = "default_decoding")]
    decoding: Box<dyn DeserializerConfig>,
    #[serde(default, deserialize_with = "bool_or_struct")]
    acknowledgements: AcknowledgementsConfig,
}

#[derive(Derivative, Copy, Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "lowercase")]
#[derivative(Default)]
pub enum Compression {
    #[derivative(Default)]
    Auto,
    None,
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
        let decoder = DecodingConfig::new(self.framing.clone(), self.decoding.clone()).build()?;
        let acknowledgements = cx.globals.acknowledgements.merge(&self.acknowledgements);

        let svc = filters::firehose(
            self.access_key.clone(),
            self.record_compression.unwrap_or_default(),
            decoder,
            acknowledgements.enabled(),
            cx.out,
        );

        let tls = MaybeTlsSettings::from_config(&self.tls, true)?;
        let listener = tls.bind(&self.address).await?;

        let shutdown = cx.shutdown;
        Ok(Box::pin(async move {
            let span = crate::trace::current_span();
            warp::serve(svc.with(warp::trace(move |_info| span.clone())))
                .serve_incoming_with_graceful_shutdown(
                    listener.accept_stream(),
                    shutdown.map(|_| ()),
                )
                .await;
            Ok(())
        }))
    }

    fn outputs(&self) -> Vec<Output> {
        vec![Output::default(DataType::Log)]
    }

    fn source_type(&self) -> &'static str {
        "aws_kinesis_firehose"
    }

    fn resources(&self) -> Vec<Resource> {
        vec![Resource::tcp(self.address)]
    }
}

inventory::submit! {
    SourceDescription::new::<AwsKinesisFirehoseConfig>("aws_kinesis_firehose")
}

impl GenerateConfig for AwsKinesisFirehoseConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(Self {
            address: "0.0.0.0:443".parse().unwrap(),
            access_key: None,
            tls: None,
            record_compression: None,
            framing: default_framing_message_based(),
            decoding: default_decoding(),
            acknowledgements: Default::default(),
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

    use bytes::Bytes;
    use chrono::{DateTime, SubsecRound, Utc};
    use flate2::read::GzEncoder;
    use futures::Stream;
    use pretty_assertions::assert_eq;
    use tokio::time::{sleep, Duration};
    use vector_common::assert_event_data_eq;

    use super::*;
    use crate::{
        event::{Event, EventStatus},
        log_event,
        test_util::{collect_ready, next_addr, wait_for_tcp},
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
        access_key: Option<String>,
        record_compression: Option<Compression>,
        delivered: bool,
    ) -> (impl Stream<Item = Event>, SocketAddr) {
        use EventStatus::*;
        let status = if delivered { Delivered } else { Rejected };
        let (sender, recv) = SourceSender::new_test_finalize(status);
        let address = next_addr();
        let cx = SourceContext::new_test(sender);
        tokio::spawn(async move {
            AwsKinesisFirehoseConfig {
                address,
                tls: None,
                access_key,
                record_compression,
                framing: default_framing_message_based(),
                decoding: default_decoding(),
                acknowledgements: true.into(),
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
            .post(&format!("http://{}", address))
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

        Ok(base64::encode(&compressed))
    }

    #[tokio::test]
    async fn aws_kinesis_firehose_forwards_events() {
        let gziped_record = {
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
                gziped_record,
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
            println!(
                "test case: ({}, {})",
                source_record_compression, record_compression
            );

            let (rx, addr) = source(None, Some(source_record_compression), true).await;

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
                        "message"=> Bytes::from(expected),
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
    async fn aws_kinesis_firehose_forwards_events_gzip_request() {
        let (rx, addr) = source(None, None, true).await;

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
    }

    #[tokio::test]
    async fn aws_kinesis_firehose_rejects_bad_access_key() {
        let (_rx, addr) = source(Some("an access key".to_string()), None, true).await;

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
    async fn handles_acknowledgement_failure() {
        let expected = RECORD.as_bytes().to_owned();

        let (rx, addr) = source(None, Some(Compression::None), false).await;

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
}
