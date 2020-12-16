use crate::{
    config::{DataType, GenerateConfig, GlobalOptions, Resource, SourceConfig, SourceDescription},
    shutdown::ShutdownSignal,
    tls::{MaybeTlsSettings, TlsConfig},
    Pipeline,
};
use futures::FutureExt;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;

pub mod errors;
mod filters;
mod handlers;
mod models;

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct AwsKinesisFirehoseConfig {
    address: SocketAddr,
    access_key: Option<String>,
    tls: Option<TlsConfig>,
}

#[async_trait::async_trait]
#[typetag::serde(name = "aws_kinesis_firehose")]
impl SourceConfig for AwsKinesisFirehoseConfig {
    async fn build(
        &self,
        _: &str,
        _: &GlobalOptions,
        shutdown: ShutdownSignal,
        out: Pipeline,
    ) -> crate::Result<super::Source> {
        let svc = filters::firehose(self.access_key.clone(), out);

        let tls = MaybeTlsSettings::from_config(&self.tls, true)?;
        let listener = tls.bind(&self.address).await?;

        Ok(Box::pin(async move {
            let _ = warp::serve(svc)
                .serve_incoming_with_graceful_shutdown(
                    listener.accept_stream(),
                    shutdown.clone().map(|_| ()),
                )
                .await;
            // We need to drop the last copy of ShutdownSignalToken only after server has shut down.
            drop(shutdown);
            Ok(())
        }))
    }

    fn output_type(&self) -> DataType {
        DataType::Log
    }

    fn source_type(&self) -> &'static str {
        "aws_kinesis_firehose"
    }

    fn resources(&self) -> Vec<Resource> {
        vec![self.address.into()]
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
        })
        .unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        event::Event,
        log_event,
        test_util::{collect_ready, next_addr, wait_for_tcp},
    };
    use chrono::{DateTime, SubsecRound, Utc};
    use flate2::{read::GzEncoder, Compression};
    use pretty_assertions::assert_eq;
    use std::{
        io::{Cursor, Read},
        net::SocketAddr,
    };
    use tokio::sync::mpsc;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<AwsKinesisFirehoseConfig>();
    }

    async fn source(access_key: Option<String>) -> (mpsc::Receiver<Event>, SocketAddr) {
        let (sender, recv) = Pipeline::new_test();
        let address = next_addr();
        tokio::spawn(async move {
            AwsKinesisFirehoseConfig {
                address,
                tls: None,
                access_key,
            }
            .build(
                "default",
                &GlobalOptions::default(),
                ShutdownSignal::noop(),
                sender,
            )
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
        records: Vec<&str>,
        key: Option<&str>,
        request_id: &str,
        source_arn: &str,
        gzip: bool,
    ) -> reqwest::Result<reqwest::Response> {
        let request = models::FirehoseRequest {
            request_id: request_id.to_string(),
            timestamp,
            records: records
                .into_iter()
                .map(|record| models::EncodedFirehoseRecord {
                    data: encode_record(record).unwrap(),
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
            .header("x-amz-firehose-request-id", request_id.to_string())
            .header("x-amz-firehose-source-arn", source_arn.to_string())
            .header("user-agent", "Amazon Kinesis Data Firehose Agent/1.0")
            .header("content-type", "application/json");

        if let Some(key) = key {
            builder = builder.header("x-amz-firehose-access-key", key);
        }

        if gzip {
            let mut gz = GzEncoder::new(
                Cursor::new(serde_json::to_vec(&request).unwrap()),
                Compression::fast(),
            );
            let mut buffer = Vec::new();
            gz.read_to_end(&mut buffer).unwrap();
            builder = builder.header("content-encoding", "gzip").body(buffer);
        } else {
            builder = builder.json(&request);
        }

        builder.send().await
    }

    /// Encodes record data to mach AWS's representation: base64 encoded, gzip'd data
    fn encode_record(record: &str) -> std::io::Result<String> {
        let mut buffer = Vec::new();

        let mut gz = GzEncoder::new(record.as_bytes(), Compression::fast());
        gz.read_to_end(&mut buffer)?;

        Ok(base64::encode(&buffer))
    }

    #[tokio::test]
    async fn aws_kinesis_firehose_forwards_events() {
        // example CloudWatch Logs subscription event
        let record = r#"
{
  "messageType": "DATA_MESSAGE",
  "owner": "071959437513",
  "logGroup": "/jesse/test",
  "logStream": "test",
  "subscriptionFilters": [
    "Destination"
  ],
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

        let (rx, addr) = source(None).await;

        let source_arn = "arn:aws:firehose:us-east-1:111111111111:deliverystream/test";
        let request_id = "e17265d6-97af-4938-982e-90d5614c4242";
        let timestamp: DateTime<Utc> = Utc::now();

        let res = send(
            addr,
            timestamp,
            vec![record],
            None,
            request_id,
            source_arn,
            false,
        )
        .await
        .unwrap();
        assert_eq!(200, res.status().as_u16());

        let events = collect_ready(rx).await;
        assert_eq!(
            events,
            vec![log_event! {
                "timestamp" => timestamp.trunc_subsecs(3), // AWS sends timestamps as ms
                "message"=> record,
                "request_id" => request_id,
                "source_arn" => source_arn,
            },]
        );

        let response: models::FirehoseResponse = res.json().await.unwrap();
        assert_eq!(response.request_id, request_id);
    }

    #[tokio::test]
    async fn aws_kinesis_firehose_forwards_events_gzip() {
        // example CloudWatch Logs subscription event
        let record = r#"
{
  "messageType": "DATA_MESSAGE",
  "owner": "071959437513",
  "logGroup": "/jesse/test",
  "logStream": "test",
  "subscriptionFilters": [
    "Destination"
  ],
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

        let (rx, addr) = source(None).await;

        let source_arn = "arn:aws:firehose:us-east-1:111111111111:deliverystream/test";
        let request_id = "e17265d6-97af-4938-982e-90d5614c4242";
        let timestamp: DateTime<Utc> = Utc::now();

        let res = send(
            addr,
            timestamp,
            vec![record],
            None,
            request_id,
            source_arn,
            true,
        )
        .await
        .unwrap();
        assert_eq!(200, res.status().as_u16());

        let events = collect_ready(rx).await;
        assert_eq!(
            events,
            vec![log_event! {
                "timestamp" => timestamp.trunc_subsecs(3), // AWS sends timestamps as ms
                "message"=> record,
                "request_id" => request_id,
                "source_arn" => source_arn,
            },]
        );

        let response: models::FirehoseResponse = res.json().await.unwrap();
        assert_eq!(response.request_id, request_id);
    }

    #[tokio::test]
    async fn aws_kinesis_firehose_rejects_bad_access_key() {
        let (_rx, addr) = source(Some("an access key".to_string())).await;

        let request_id = "e17265d6-97af-4938-982e-90d5614c4242";

        let res = send(
            addr,
            Utc::now(),
            vec![],
            Some("bad access key"),
            request_id,
            "",
            false,
        )
        .await
        .unwrap();
        assert_eq!(401, res.status().as_u16());

        let response: models::FirehoseResponse = res.json().await.unwrap();
        assert_eq!(response.request_id, request_id);
    }
}
