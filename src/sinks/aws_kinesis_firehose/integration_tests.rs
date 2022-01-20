#![cfg(feature = "aws-kinesis-firehose-integration-tests")]
#![cfg(test)]

use futures::{StreamExt, TryFutureExt};
use rusoto_core::Region;
use rusoto_es::{CreateElasticsearchDomainRequest, Es, EsClient};
use rusoto_firehose::{
    CreateDeliveryStreamInput, ElasticsearchDestinationConfiguration, KinesisFirehose,
    KinesisFirehoseClient,
};
use serde_json::{json, Value};
use tokio::time::{sleep, Duration};

use super::*;
use crate::sinks::elasticsearch::BulkConfig;
use crate::{
    aws::{AwsAuthentication, RegionOrEndpoint},
    config::{SinkConfig, SinkContext},
    sinks::{
        elasticsearch::{ElasticSearchAuth, ElasticSearchCommon, ElasticSearchConfig},
        util::{
            encoding::{EncodingConfig, StandardEncodings},
            BatchConfig, Compression, TowerRequestConfig,
        },
    },
    test_util::{
        components, components::AWS_SINK_TAGS, random_events_with_stream, random_string,
        wait_for_duration,
    },
};

fn kinesis_address() -> String {
    std::env::var("KINESIS_ADDRESS").unwrap_or_else(|_| "http://localhost:4566".into())
}

fn elasticsearch_address() -> String {
    std::env::var("ELASTICSEARCH_ADDRESS").unwrap_or_else(|_| "http://localhost:4571".into())
}

#[tokio::test]
async fn firehose_put_records() {
    let stream = gen_stream();

    let region = Region::Custom {
        name: "localstack".into(),
        endpoint: kinesis_address(),
    };

    let elasticseacrh_arn = ensure_elasticsearch_domain(region.clone(), stream.clone()).await;

    ensure_elasticesarch_delivery_stream(region, stream.clone(), elasticseacrh_arn.clone()).await;

    let mut batch = BatchConfig::default();
    batch.max_events = Some(2);

    let config = KinesisFirehoseSinkConfig {
        stream_name: stream.clone(),
        region: RegionOrEndpoint::with_endpoint(kinesis_address().as_str()),
        encoding: EncodingConfig::from(StandardEncodings::Json), // required for ES destination w/ localstack
        compression: Compression::None,
        batch,
        request: TowerRequestConfig {
            timeout_secs: Some(10),
            retry_attempts: Some(0),
            ..Default::default()
        },
        assume_role: None,
        auth: Default::default(),
    };

    let cx = SinkContext::new_test();

    let sink = config.build(cx).await.unwrap();

    let (input, events) = random_events_with_stream(100, 100, None);

    components::init_test();
    sink.0.run(events.map(Into::into)).await.unwrap();

    sleep(Duration::from_secs(5)).await;
    components::SINK_TESTS.assert(&AWS_SINK_TAGS);

    let config = ElasticSearchConfig {
        auth: Some(ElasticSearchAuth::Aws(AwsAuthentication::Default {})),
        endpoint: elasticsearch_address(),
        bulk: Some(BulkConfig {
            index: Some(stream.clone()),
            action: None,
        }),
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
    #[allow(clippy::needless_collect)] // https://github.com/rust-lang/rust-clippy/issues/6909
    let input = input
        .into_iter()
        .map(|rec| serde_json::to_value(&rec.into_log()).unwrap())
        .collect::<Vec<_>>();
    for hit in hits {
        let hit = hit
            .get("_source")
            .expect("Elasticsearch hit missing _source");
        assert!(input.contains(hit));
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
            reqwest::get(format!("{}/_cluster/health", elasticsearch_address()))
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
        Duration::from_secs(60),
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
