use aws_sdk_elasticsearch::{types::DomainEndpointOptions, Client as EsClient};
use aws_sdk_firehose::types::ElasticsearchDestinationConfiguration;
use futures::StreamExt;
use futures::TryFutureExt;
use serde_json::{json, Value};
use tokio::time::{sleep, Duration};
use vector_lib::codecs::JsonSerializerConfig;
use vector_lib::lookup::lookup_v2::ConfigValuePath;

use super::{config::KinesisFirehoseClientBuilder, *};
use crate::{
    aws::{create_client, AwsAuthentication, ImdsAuthentication, RegionOrEndpoint},
    config::{ProxyConfig, SinkConfig, SinkContext},
    sinks::{
        elasticsearch::{
            BulkConfig, ElasticsearchAuthConfig, ElasticsearchCommon, ElasticsearchConfig,
        },
        util::{BatchConfig, Compression, TowerRequestConfig},
    },
    template::Template,
    test_util::{
        components::{run_and_assert_sink_compliance, AWS_SINK_TAGS},
        random_events_with_stream, random_string, wait_for_duration,
    },
};

fn kinesis_address() -> String {
    std::env::var("KINESIS_ADDRESS").unwrap_or_else(|_| "http://localhost:4566".into())
}

fn elasticsearch_address() -> String {
    format!(
        "{}/es-endpoint",
        std::env::var("ELASTICSEARCH_ADDRESS").unwrap_or_else(|_| "http://localhost:4566".into()),
    )
}

#[tokio::test]
async fn firehose_put_records_without_partition_key() {
    let stream = gen_stream();

    let elasticsearch_arn = ensure_elasticsearch_domain(stream.clone().to_string()).await;

    ensure_elasticsearch_delivery_stream(stream.clone(), elasticsearch_arn.clone()).await;

    let mut batch = BatchConfig::default();
    batch.max_events = Some(2);

    let region = RegionOrEndpoint::with_both("us-east-1", kinesis_address().as_str());

    let base = KinesisSinkBaseConfig {
        stream_name: stream.clone(),
        region: region.clone(),
        encoding: JsonSerializerConfig::default().into(), // required for ES destination w/ localstack
        compression: Compression::None,
        request: TowerRequestConfig {
            timeout_secs: 10,
            retry_attempts: 0,
            ..Default::default()
        },
        tls: None,
        auth: Default::default(),
        acknowledgements: Default::default(),
        request_retry_partial: Default::default(),
        partition_key_field: None,
    };

    let config = KinesisFirehoseSinkConfig { batch, base };

    let cx = SinkContext::default();

    let (sink, _) = config.build(cx).await.unwrap();

    let (input, events) = random_events_with_stream(100, 100, None);

    run_and_assert_sink_compliance(sink, events, &AWS_SINK_TAGS).await;

    // Hard-coded sleeps are bad, but we're waiting on localstack's state to converge.
    sleep(Duration::from_secs(5)).await;

    let config = ElasticsearchConfig {
        auth: Some(ElasticsearchAuthConfig::Aws(AwsAuthentication::Default {
            load_timeout_secs: Some(5),
            imds: ImdsAuthentication::default(),
            region: None,
        })),
        endpoints: vec![elasticsearch_address()],
        bulk: BulkConfig {
            index: Template::try_from(stream.clone()).expect("unable to parse Template"),
            ..Default::default()
        },
        aws: Some(region),
        ..Default::default()
    };
    let common = ElasticsearchCommon::parse_single(&config)
        .await
        .expect("Config error");

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
        .map(|rec| serde_json::to_value(rec.into_log()).unwrap())
        .collect::<Vec<_>>();
    for hit in hits {
        let hit = hit
            .get("_source")
            .expect("Elasticsearch hit missing _source");
        assert!(input.contains(hit));
    }
}

#[tokio::test]
async fn firehose_put_records_with_partition_key() {
    let stream = gen_stream();

    let elasticsearch_arn = ensure_elasticsearch_domain(stream.clone().to_string()).await;

    ensure_elasticsearch_delivery_stream(stream.clone(), elasticsearch_arn.clone()).await;

    let mut batch = BatchConfig::default();
    batch.max_events = Some(20);

    let region = RegionOrEndpoint::with_both("us-east-1", kinesis_address().as_str());

    let partition_value = "a_value";
    let partition_key = ConfigValuePath::try_from("partition_key".to_string()).unwrap();

    let base = KinesisSinkBaseConfig {
        stream_name: stream.clone(),
        region: region.clone(),
        encoding: JsonSerializerConfig::default().into(), // required for ES destination w/ localstack
        compression: Compression::None,
        request: TowerRequestConfig {
            timeout_secs: 10,
            retry_attempts: 0,
            ..Default::default()
        },
        tls: None,
        auth: Default::default(),
        acknowledgements: Default::default(),
        request_retry_partial: Default::default(),
        partition_key_field: Some(partition_key.clone()),
    };

    let config = KinesisFirehoseSinkConfig { batch, base };

    let cx = SinkContext::default();

    let (sink, _) = config.build(cx).await.unwrap();

    let (mut input, events) = random_events_with_stream(100, 100, None);

    let events = events.map(move |mut events| {
        events.iter_logs_mut().for_each(move |log| {
            log.insert("partition_key", partition_value);
        });
        events
    });

    input.iter_mut().for_each(move |log| {
        log.as_mut_log().insert("partition_key", partition_value);
    });

    run_and_assert_sink_compliance(sink, events, &AWS_SINK_TAGS).await;

    // Hard-coded sleeps are bad, but we're waiting on localstack's state to converge.
    sleep(Duration::from_secs(5)).await;

    let config = ElasticsearchConfig {
        auth: Some(ElasticsearchAuthConfig::Aws(AwsAuthentication::Default {
            load_timeout_secs: Some(5),
            imds: ImdsAuthentication::default(),
            region: None,
        })),
        endpoints: vec![elasticsearch_address()],
        bulk: BulkConfig {
            index: Template::try_from(stream.clone()).expect("unable to parse Template"),
            ..Default::default()
        },
        aws: Some(region),
        ..Default::default()
    };
    let common = ElasticsearchCommon::parse_single(&config)
        .await
        .expect("Config error");

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
        .map(|rec| serde_json::to_value(rec.into_log()).unwrap())
        .collect::<Vec<_>>();
    for hit in hits {
        let hit = hit
            .get("_source")
            .expect("Elasticsearch hit missing _source");
        assert!(input.contains(hit));
    }
}

fn test_region_endpoint() -> RegionOrEndpoint {
    RegionOrEndpoint::with_both("us-east-1", kinesis_address())
}

async fn firehose_client() -> aws_sdk_firehose::Client {
    let region_endpoint = test_region_endpoint();
    let auth = AwsAuthentication::test_auth();
    let proxy = ProxyConfig::default();

    create_client::<KinesisFirehoseClientBuilder>(
        &auth,
        region_endpoint.region(),
        region_endpoint.endpoint(),
        &proxy,
        &None,
        &None,
    )
    .await
    .unwrap()
}

/// creates ES domain with the given name and returns the ARN
async fn ensure_elasticsearch_domain(domain_name: String) -> String {
    let client = EsClient::from_conf(
        aws_sdk_elasticsearch::config::Builder::new()
            .credentials_provider(
                AwsAuthentication::test_auth()
                    .credentials_provider(
                        test_region_endpoint().region().unwrap(),
                        &Default::default(),
                        &None,
                    )
                    .await
                    .unwrap(),
            )
            .endpoint_url(test_region_endpoint().endpoint().unwrap())
            .region(test_region_endpoint().region())
            .build(),
    );

    let arn = match client
        .create_elasticsearch_domain()
        .domain_name(domain_name)
        .domain_endpoint_options(
            DomainEndpointOptions::builder()
                .custom_endpoint_enabled(true)
                .custom_endpoint(elasticsearch_address())
                .build(),
        )
        .send()
        .await
    {
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
                        .map(|status| status != "red")
                        .unwrap_or(false)
                })
                .unwrap_or(false)
        },
        Duration::from_secs(120),
    )
    .await;

    arn
}

/// creates Firehose delivery stream to ship to Elasticsearch
async fn ensure_elasticsearch_delivery_stream(
    delivery_stream_name: String,
    elasticsearch_arn: String,
) {
    let client = firehose_client().await;

    match client
        .create_delivery_stream()
        .delivery_stream_name(delivery_stream_name.clone())
        .elasticsearch_destination_configuration(
            ElasticsearchDestinationConfiguration::builder()
                .index_name(delivery_stream_name)
                .domain_arn(elasticsearch_arn)
                .role_arn("doesn't matter")
                .type_name("doesn't matter")
                .build()
                .expect("all builder fields populated"),
        )
        .send()
        .await
    {
        Ok(_) => (),
        Err(error) => panic!("Unable to create the delivery stream {:?}", error),
    };
}

fn gen_stream() -> String {
    format!("test-{}", random_string(10).to_lowercase())
}
