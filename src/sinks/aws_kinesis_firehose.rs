use crate::{
    dns::Resolver,
    event::{self, Event},
    region::RegionOrEndpoint,
    sinks::util::{
        encoding::{EncodingConfig, EncodingConfiguration},
        retries::RetryLogic,
        rusoto2::{self, AwsCredentialsProvider},
        BatchEventsConfig, TowerRequestConfig,
    },
    topology::config::{DataType, SinkConfig, SinkContext, SinkDescription},
};
use bytes::Bytes;
use futures01::{stream::iter_ok, Future, Poll, Sink};
use lazy_static::lazy_static;
use rusoto_core::{Region, RusotoError, RusotoFuture};
use rusoto_firehose::{
    DescribeDeliveryStreamInput, KinesisFirehose, KinesisFirehoseClient, PutRecordBatchError,
    PutRecordBatchInput, PutRecordBatchOutput, Record,
};
use serde::{Deserialize, Serialize};
use snafu::Snafu;
use std::{convert::TryInto, fmt};
use tower::Service;
use tracing_futures::{Instrument, Instrumented};

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
    pub batch: BatchEventsConfig,
    #[serde(default)]
    pub request: TowerRequestConfig,
    pub assume_role: Option<String>,
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
    SinkDescription::new_without_default::<KinesisFirehoseSinkConfig>("aws_kinesis_firehose")
}

#[typetag::serde(name = "aws_kinesis_firehose")]
impl SinkConfig for KinesisFirehoseSinkConfig {
    fn build(&self, cx: SinkContext) -> crate::Result<(super::RouterSink, super::Healthcheck)> {
        let config = self.clone();
        let healthcheck = healthcheck(self.clone(), cx.resolver())?;
        let sink = KinesisFirehoseService::new(config, cx)?;
        Ok((Box::new(sink), healthcheck))
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }

    fn sink_type(&self) -> &'static str {
        "aws_kinesis_firehose"
    }
}

impl KinesisFirehoseService {
    pub fn new(
        config: KinesisFirehoseSinkConfig,
        cx: SinkContext,
    ) -> crate::Result<impl Sink<SinkItem = Event, SinkError = ()>> {
        let client = create_client(
            config.region.clone().try_into()?,
            config.assume_role.clone(),
            cx.resolver(),
        )?;

        let batch = config.batch.unwrap_or(500, 1);
        let request = config.request.unwrap_with(&REQUEST_DEFAULTS);
        let encoding = config.encoding.clone();

        let kinesis = KinesisFirehoseService { client, config };

        let sink = request
            .batch_sink(
                KinesisFirehoseRetryLogic,
                kinesis,
                Vec::new(),
                batch,
                cx.acker(),
            )
            .sink_map_err(|e| error!("Fatal kinesis firehose sink error: {}", e))
            .with_flat_map(move |e| iter_ok(encode_event(e, &encoding)));

        Ok(sink)
    }
}

impl Service<Vec<Record>> for KinesisFirehoseService {
    type Response = PutRecordBatchOutput;
    type Error = RusotoError<PutRecordBatchError>;
    type Future = Instrumented<RusotoFuture<PutRecordBatchOutput, PutRecordBatchError>>;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        Ok(().into())
    }

    fn call(&mut self, records: Vec<Record>) -> Self::Future {
        debug!(
            message = "sending records.",
            events = %records.len(),
        );

        let request = PutRecordBatchInput {
            records,
            delivery_stream_name: self.config.stream_name.clone(),
        };

        self.client
            .put_record_batch(request)
            .instrument(info_span!("request"))
    }
}

impl fmt::Debug for KinesisFirehoseService {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("KinesisFirehoseService")
            .field("config", &self.config)
            .finish()
    }
}

#[derive(Debug, Clone)]
struct KinesisFirehoseRetryLogic;

impl RetryLogic for KinesisFirehoseRetryLogic {
    type Error = RusotoError<PutRecordBatchError>;
    type Response = PutRecordBatchOutput;

    fn is_retriable_error(&self, error: &Self::Error) -> bool {
        match error {
            RusotoError::HttpDispatch(_) => true,
            RusotoError::Service(PutRecordBatchError::ServiceUnavailable(_)) => true,
            RusotoError::Unknown(res) if res.status.is_server_error() => true,
            _ => false,
        }
    }
}

#[derive(Debug, Snafu)]
enum HealthcheckError {
    #[snafu(display("DescribeDeliveryStream failed: {}", source))]
    DescribeDeliveryStreamFailed {
        source: RusotoError<rusoto_firehose::DescribeDeliveryStreamError>,
    },
    #[snafu(display("Stream name does not match, got {}, expected {}", name, stream_name))]
    StreamNamesMismatch { name: String, stream_name: String },
}

fn healthcheck(
    config: KinesisFirehoseSinkConfig,
    resolver: Resolver,
) -> crate::Result<super::Healthcheck> {
    let client = create_client(config.region.try_into()?, config.assume_role, resolver)?;
    let stream_name = config.stream_name;

    let fut = client
        .describe_delivery_stream(DescribeDeliveryStreamInput {
            delivery_stream_name: stream_name.clone(),
            exclusive_start_destination_id: None,
            limit: Some(1),
        })
        .map_err(|source| HealthcheckError::DescribeDeliveryStreamFailed { source }.into())
        .and_then(move |res| Ok(res.delivery_stream_description.delivery_stream_name))
        .and_then(move |name| {
            if name == stream_name {
                Ok(())
            } else {
                Err(HealthcheckError::StreamNamesMismatch { name, stream_name }.into())
            }
        });

    Ok(Box::new(fut))
}

fn create_client(
    region: Region,
    assume_role: Option<String>,
    resolver: Resolver,
) -> crate::Result<KinesisFirehoseClient> {
    let client = rusoto2::client(resolver)?;
    let creds = AwsCredentialsProvider::new(&region, assume_role)?;

    Ok(KinesisFirehoseClient::new_with(client, creds, region))
}

fn encode_event(mut event: Event, encoding: &EncodingConfig<Encoding>) -> Option<Record> {
    encoding.apply_rules(&mut event);
    let log = event.into_log();
    let data = match encoding.codec() {
        Encoding::Json => serde_json::to_vec(&log).expect("Error encoding event as json."),

        Encoding::Text => log
            .get(&event::log_schema().message_key())
            .map(|v| v.as_bytes().to_vec())
            .unwrap_or_default(),
    };

    let data = Bytes::from(data);

    Some(Record { data })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{self, Event};
    use std::collections::BTreeMap;

    #[test]
    fn firehose_encode_event_text() {
        let message = "hello world".to_string();
        let event = encode_event(message.clone().into(), &Encoding::Text.into()).unwrap();

        assert_eq!(&event.data[..], message.as_bytes());
    }

    #[test]
    fn firehose_encode_event_json() {
        let message = "hello world".to_string();
        let mut event = Event::from(message.clone());
        event.as_mut_log().insert("key", "value");
        let event = encode_event(event, &Encoding::Json.into()).unwrap();

        let map: BTreeMap<String, String> = serde_json::from_slice(&event.data[..]).unwrap();

        assert_eq!(map[&event::log_schema().message_key().to_string()], message);
        assert_eq!(map["key"], "value".to_string());
    }
}

#[cfg(feature = "aws-kinesis-firehose-integration-tests")]
#[cfg(test)]
mod integration_tests {
    use super::*;
    use crate::{
        region::RegionOrEndpoint,
        runtime,
        sinks::{
            elasticsearch::{ElasticSearchAuth, ElasticSearchCommon, ElasticSearchConfig},
            util::BatchEventsConfig,
        },
        test_util::{random_events_with_stream, random_string},
        topology::config::SinkContext,
    };
    use futures01::Sink;
    use rusoto_core::Region;
    use rusoto_firehose::{
        CreateDeliveryStreamInput, ElasticsearchDestinationConfiguration, KinesisFirehose,
        KinesisFirehoseClient,
    };
    use serde_json::{json, Value};
    use std::{thread, time::Duration};

    #[test]
    fn firehose_put_records() {
        let stream = gen_stream();

        let region = Region::Custom {
            name: "localstack".into(),
            endpoint: "http://localhost:4573".into(),
        };

        ensure_stream(region.clone(), stream.clone());

        let config = KinesisFirehoseSinkConfig {
            stream_name: stream.clone(),
            region: RegionOrEndpoint::with_endpoint("http://localhost:4573".into()),
            encoding: EncodingConfig::from(Encoding::Json), // required for ES destination w/ localstack
            batch: BatchEventsConfig {
                max_events: Some(2),
                timeout_secs: None,
            },
            request: TowerRequestConfig {
                timeout_secs: Some(10),
                retry_attempts: Some(0),
                ..Default::default()
            },
            assume_role: None,
        };

        let mut rt = runtime::Runtime::new().unwrap();
        let cx = SinkContext::new_test(rt.executor());

        let sink = KinesisFirehoseService::new(config, cx).unwrap();

        let (input, events) = random_events_with_stream(100, 100);

        let pump = sink.send_all(events);
        let _ = rt.block_on(pump).unwrap();

        thread::sleep(Duration::from_secs(1));

        let config = ElasticSearchConfig {
            auth: Some(ElasticSearchAuth::Aws),
            host: "http://localhost:4571".into(),
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
            .unwrap()
            .json::<elastic_responses::search::SearchResponse<Value>>()
            .unwrap();

        assert_eq!(input.len() as u64, response.total());
        let input = input
            .into_iter()
            .map(|rec| serde_json::to_value(&rec.into_log()).unwrap())
            .collect::<Vec<_>>();
        for hit in response.into_hits() {
            let event = hit.into_document().unwrap();
            assert!(input.contains(&event));
        }
    }

    fn ensure_stream(region: Region, delivery_stream_name: String) {
        let client = KinesisFirehoseClient::new(region);

        let es_config = ElasticsearchDestinationConfiguration {
            index_name: delivery_stream_name.clone(),
            domain_arn: "doesn't matter".into(),
            role_arn: "doesn't matter".into(),
            type_name: "doesn't matter".into(),
            ..Default::default()
        };

        let req = CreateDeliveryStreamInput {
            delivery_stream_name,
            elasticsearch_destination_configuration: Some(es_config),
            ..Default::default()
        };

        match client.create_delivery_stream(req).sync() {
            Ok(_) => (),
            Err(e) => println!("Unable to create the delivery stream {:?}", e),
        };
    }

    fn gen_stream() -> String {
        format!("test-{}", random_string(10).to_lowercase())
    }
}
