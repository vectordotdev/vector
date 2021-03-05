use crate::{
    config::{DataType, GenerateConfig, SinkConfig, SinkContext, SinkDescription},
    event::Event,
    rusoto::{self, AWSAuthentication, RegionOrEndpoint},
    sinks::util::{
        encoding::{EncodingConfig, EncodingConfiguration},
        retries::RetryLogic,
        sink::Response,
        BatchConfig, BatchSettings, Compression, EncodedLength, TowerRequestConfig, VecBuffer,
    },
};
use bytes::Bytes;
use futures::{future::BoxFuture, stream, FutureExt, Sink, SinkExt, StreamExt};
use lazy_static::lazy_static;
use rusoto_core::RusotoError;
use rusoto_firehose::{
    DescribeDeliveryStreamError, DescribeDeliveryStreamInput, KinesisFirehose,
    KinesisFirehoseClient, PutRecordBatchError, PutRecordBatchInput, PutRecordBatchOutput, Record,
};
use serde::{Deserialize, Serialize};
use snafu::Snafu;
use std::{
    convert::TryInto,
    fmt,
    task::{Context, Poll},
};
use tower::Service;
use tracing_futures::Instrument;

#[derive(Clone)]
pub struct KinesisFirehoseService {
    client: KinesisFirehoseClient,
    config: KinesisFirehoseSinkConfig,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct KinesisFirehoseSinkConfig {
    pub stream_name: String,
    #[serde(flatten)]
    pub region: RegionOrEndpoint,
    pub encoding: EncodingConfig<Encoding>,
    #[serde(default)]
    pub compression: Compression,
    #[serde(default)]
    pub batch: BatchConfig,
    #[serde(default)]
    pub request: TowerRequestConfig,
    // Deprecated name. Moved to auth.
    assume_role: Option<String>,
    #[serde(default)]
    pub auth: AWSAuthentication,
}

lazy_static! {
    static ref REQUEST_DEFAULTS: TowerRequestConfig = TowerRequestConfig {
        timeout_secs: Some(30),
        ..Default::default()
    };
}

#[derive(Deserialize, Serialize, Debug, Eq, PartialEq, Clone)]
#[serde(rename_all = "snake_case")]
pub enum Encoding {
    Text,
    Json,
}

inventory::submit! {
    SinkDescription::new::<KinesisFirehoseSinkConfig>("aws_kinesis_firehose")
}

impl GenerateConfig for KinesisFirehoseSinkConfig {
    fn generate_config() -> toml::Value {
        toml::from_str(
            r#"region = "us-east-1"
            stream_name = "my-stream"
            encoding.codec = "json""#,
        )
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "aws_kinesis_firehose")]
impl SinkConfig for KinesisFirehoseSinkConfig {
    async fn build(
        &self,
        cx: SinkContext,
    ) -> crate::Result<(super::VectorSink, super::Healthcheck)> {
        let client = self.create_client()?;
        let healthcheck = self.clone().healthcheck(client.clone()).boxed();
        let sink = KinesisFirehoseService::new(self.clone(), client, cx)?;
        Ok((super::VectorSink::Sink(Box::new(sink)), healthcheck))
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }

    fn sink_type(&self) -> &'static str {
        "aws_kinesis_firehose"
    }
}

impl KinesisFirehoseSinkConfig {
    async fn healthcheck(self, client: KinesisFirehoseClient) -> crate::Result<()> {
        let stream_name = self.stream_name;

        let req = client.describe_delivery_stream(DescribeDeliveryStreamInput {
            delivery_stream_name: stream_name.clone(),
            exclusive_start_destination_id: None,
            limit: Some(1),
        });

        match req.await {
            Ok(resp) => {
                let name = resp.delivery_stream_description.delivery_stream_name;
                if name == stream_name {
                    Ok(())
                } else {
                    Err(HealthcheckError::StreamNamesMismatch { name, stream_name }.into())
                }
            }
            Err(source) => Err(HealthcheckError::DescribeDeliveryStreamFailed { source }.into()),
        }
    }

    fn create_client(&self) -> crate::Result<KinesisFirehoseClient> {
        let region = (&self.region).try_into()?;

        let client = rusoto::client()?;
        let creds = self.auth.build(&region, self.assume_role.clone())?;

        let client = rusoto_core::Client::new_with_encoding(creds, client, self.compression.into());
        Ok(KinesisFirehoseClient::new_with_client(client, region))
    }
}

impl KinesisFirehoseService {
    pub fn new(
        config: KinesisFirehoseSinkConfig,
        client: KinesisFirehoseClient,
        cx: SinkContext,
    ) -> crate::Result<impl Sink<Event, Error = ()>> {
        let batch = BatchSettings::default()
            .bytes(4_000_000)
            .events(500)
            .timeout(1)
            .parse_config(config.batch)?;
        let request = config.request.unwrap_with(&REQUEST_DEFAULTS);
        let encoding = config.encoding.clone();

        let kinesis = KinesisFirehoseService { client, config };

        let sink = request
            .batch_sink(
                KinesisFirehoseRetryLogic,
                kinesis,
                VecBuffer::new(batch.size),
                batch.timeout,
                cx.acker(),
            )
            .sink_map_err(|error| error!(message = "Fatal kinesis firehose sink error.", %error))
            .with_flat_map(move |e| stream::iter(Some(encode_event(e, &encoding))).map(Ok));

        Ok(sink)
    }
}

impl Service<Vec<Record>> for KinesisFirehoseService {
    type Response = PutRecordBatchOutput;
    type Error = RusotoError<PutRecordBatchError>;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _cx: &mut Context) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, records: Vec<Record>) -> Self::Future {
        debug!(
            message = "Sending records.",
            events = %records.len(),
        );

        let client = self.client.clone();
        let request = PutRecordBatchInput {
            records,
            delivery_stream_name: self.config.stream_name.clone(),
        };

        Box::pin(async move {
            client
                .put_record_batch(request)
                .instrument(info_span!("request"))
                .await
        })
    }
}

impl EncodedLength for Record {
    fn encoded_length(&self) -> usize {
        // data is simply base64 encoded, quoted, and comma separated
        (self.data.len() + 2) / 3 * 4 + 3
    }
}

impl fmt::Debug for KinesisFirehoseService {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("KinesisFirehoseService")
            .field("config", &self.config)
            .finish()
    }
}

impl Response for PutRecordBatchOutput {}

#[derive(Debug, Clone)]
struct KinesisFirehoseRetryLogic;

impl RetryLogic for KinesisFirehoseRetryLogic {
    type Error = RusotoError<PutRecordBatchError>;
    type Response = PutRecordBatchOutput;

    fn is_retriable_error(&self, error: &Self::Error) -> bool {
        match error {
            RusotoError::Service(PutRecordBatchError::ServiceUnavailable(_)) => true,
            error => rusoto::is_retriable_error(error),
        }
    }
}

#[derive(Debug, Snafu)]
enum HealthcheckError {
    #[snafu(display("DescribeDeliveryStream failed: {}", source))]
    DescribeDeliveryStreamFailed {
        source: RusotoError<DescribeDeliveryStreamError>,
    },
    #[snafu(display("Stream name does not match, got {}, expected {}", name, stream_name))]
    StreamNamesMismatch { name: String, stream_name: String },
}

fn encode_event(mut event: Event, encoding: &EncodingConfig<Encoding>) -> Record {
    encoding.apply_rules(&mut event);
    let log = event.into_log();
    let data = match encoding.codec() {
        Encoding::Json => serde_json::to_vec(&log).expect("Error encoding event as json."),

        Encoding::Text => log
            .get(crate::config::log_schema().message_key())
            .map(|v| v.as_bytes().to_vec())
            .unwrap_or_default(),
    };

    let data = Bytes::from(data);

    Record { data }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<KinesisFirehoseSinkConfig>();
    }

    #[test]
    fn firehose_encode_event_text() {
        let message = "hello world".to_string();
        let event = encode_event(message.clone().into(), &Encoding::Text.into());

        assert_eq!(&event.data[..], message.as_bytes());
    }

    #[test]
    fn firehose_encode_event_json() {
        let message = "hello world".to_string();
        let mut event = Event::from(message.clone());
        event.as_mut_log().insert("key", "value");
        let event = encode_event(event, &Encoding::Json.into());

        let map: BTreeMap<String, String> = serde_json::from_slice(&event.data[..]).unwrap();

        assert_eq!(
            map[&crate::config::log_schema().message_key().to_string()],
            message
        );
        assert_eq!(map["key"], "value".to_string());
    }
}

#[cfg(feature = "aws-kinesis-firehose-integration-tests")]
#[cfg(test)]
mod integration_tests {
    use super::*;
    use crate::{
        sinks::elasticsearch::{ElasticSearchAuth, ElasticSearchCommon, ElasticSearchConfig},
        test_util::{random_events_with_stream, random_string, wait_for_duration},
    };
    use futures::TryFutureExt;
    use rusoto_core::Region;
    use rusoto_es::{CreateElasticsearchDomainRequest, Es, EsClient};
    use rusoto_firehose::{CreateDeliveryStreamInput, ElasticsearchDestinationConfiguration};
    use serde_json::{json, Value};
    use tokio::time::{delay_for, Duration};

    #[tokio::test]
    async fn firehose_put_records() {
        let stream = gen_stream();

        let region = Region::Custom {
            name: "localstack".into(),
            endpoint: "http://localhost:4566".into(),
        };

        let elasticseacrh_arn = ensure_elasticsearch_domain(region.clone(), stream.clone()).await;

        ensure_elasticesarch_delivery_stream(region, stream.clone(), elasticseacrh_arn.clone())
            .await;

        let config = KinesisFirehoseSinkConfig {
            stream_name: stream.clone(),
            region: RegionOrEndpoint::with_endpoint("http://localhost:4566".into()),
            encoding: EncodingConfig::from(Encoding::Json), // required for ES destination w/ localstack
            compression: Compression::None,
            batch: BatchConfig {
                max_events: Some(2),
                ..Default::default()
            },
            request: TowerRequestConfig {
                timeout_secs: Some(10),
                retry_attempts: Some(0),
                ..Default::default()
            },
            assume_role: None,
            auth: Default::default(),
        };

        let cx = SinkContext::new_test();

        let client = config.create_client().unwrap();
        let mut sink = KinesisFirehoseService::new(config, client, cx).unwrap();

        let (input, events) = random_events_with_stream(100, 100);
        let mut events = events.map(Ok);

        let _ = sink.send_all(&mut events).await.unwrap();

        delay_for(Duration::from_secs(1)).await;

        let config = ElasticSearchConfig {
            auth: Some(ElasticSearchAuth::Aws(AWSAuthentication::Default {})),
            endpoint: "http://localhost:4571".into(),
            index: Some(stream.clone()),
            ..Default::default()
        };
        let common = ElasticSearchCommon::parse_config(&config).expect("Config error");

        let client = reqwest::Client::builder()
            .build()
            .expect("Could not build HTTP client");

        let response = client
            .get(&format!("{}/{}/_search", common.base_url, stream))
            .json(&json!({
                "query": { "query_string": { "query": "*" } }
            }))
            .send()
            .await
            .unwrap()
            .json::<Value>()
            .await
            .expect("could not issue Elasticsearch search request");

        let total = response["hits"]["total"]["value"]
            .as_u64()
            .expect("Elasticsearch response does not include hits->total->value");
        assert_eq!(input.len() as u64, total);

        let hits = response["hits"]["hits"]
            .as_array()
            .expect("Elasticsearch response does not include hits->hits");
        let input = input
            .into_iter()
            .map(|rec| serde_json::to_value(&rec.into_log()).unwrap())
            .collect::<Vec<_>>();
        for hit in hits {
            let hit = hit
                .get("_source")
                .expect("Elasticsearch hit missing _source");
            assert!(input.contains(&hit));
        }
    }

    /// creates ES domain with the given name and returns the ARN
    async fn ensure_elasticsearch_domain(region: Region, domain_name: String) -> String {
        let client = EsClient::new(region);

        let req = CreateElasticsearchDomainRequest {
            domain_name,
            ..Default::default()
        };

        let arn = match client.create_elasticsearch_domain(req).await {
            Ok(res) => res.domain_status.expect("no domain status").arn,
            Err(error) => panic!("Unable to create the Elasticsearch domain {:?}", error),
        };

        // wait for ES to be available; it starts up when the ES domain is created
        // This takes a long time
        wait_for_duration(
            || async {
                reqwest::get("http://localhost:4571/_cluster/health")
                    .and_then(reqwest::Response::json::<Value>)
                    .await
                    .map(|v| {
                        v.get("status")
                            .and_then(|status| status.as_str())
                            .map(|status| status == "green")
                            .unwrap_or(false)
                    })
                    .unwrap_or(false)
            },
            Duration::from_secs(30),
        )
        .await;

        arn
    }

    /// creates Firehose delivery stream to ship to Elasticsearch
    async fn ensure_elasticesarch_delivery_stream(
        region: Region,
        delivery_stream_name: String,
        elasticseacrh_arn: String,
    ) {
        let client = KinesisFirehoseClient::new(region);

        let es_config = ElasticsearchDestinationConfiguration {
            index_name: delivery_stream_name.clone(),
            domain_arn: Some(elasticseacrh_arn),
            role_arn: "doesn't matter".into(),
            type_name: Some("doesn't matter".into()),
            ..Default::default()
        };

        let req = CreateDeliveryStreamInput {
            delivery_stream_name,
            elasticsearch_destination_configuration: Some(es_config),
            ..Default::default()
        };

        match client.create_delivery_stream(req).await {
            Ok(_) => (),
            Err(error) => panic!("Unable to create the delivery stream {:?}", error),
        };
    }

    fn gen_stream() -> String {
        format!("test-{}", random_string(10).to_lowercase())
    }
}
