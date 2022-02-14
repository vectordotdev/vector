use std::{fs::File, io::Read};

use chrono::Utc;
use futures::StreamExt;
use http::{Request, StatusCode};
use hyper::Body;
use serde_json::{json, Value};
use vector_core::{
    config::log_schema,
    event::{BatchNotifier, BatchStatus, LogEvent},
};

use super::{config::DATA_STREAM_TIMESTAMP_KEY, *};
use crate::{
    config::{ProxyConfig, SinkConfig, SinkContext},
    http::HttpClient,
    sinks::{
        util::{BatchConfig, Compression},
        HealthcheckError,
    },
    test_util::{random_events_with_stream, random_string, trace_init},
    tls::{self, TlsOptions},
};

fn aws_server() -> String {
    std::env::var("ELASTICSEARCH_AWS_ADDRESS").unwrap_or_else(|_| "http://localhost:4571".into())
}

fn http_server() -> String {
    std::env::var("ELASTICSEARCH_HTTP_ADDRESS").unwrap_or_else(|_| "http://localhost:9200".into())
}

fn https_server() -> String {
    std::env::var("ELASTICSEARCH_HTTPS_ADDRESS").unwrap_or_else(|_| "https://localhost:9201".into())
}

impl ElasticSearchCommon {
    async fn flush_request(&self) -> crate::Result<()> {
        let url = format!("{}/_flush", self.base_url)
            .parse::<hyper::Uri>()
            .unwrap();
        let mut builder = Request::post(&url);

        if let Some(credentials_provider) = &self.credentials {
            let mut request = self.signed_request("POST", &url, true);

            if let Some(ce) = self.compression.content_encoding() {
                request.add_header("Content-Encoding", ce);
            }

            for (header, value) in &self.request.headers {
                request.add_header(header, value);
            }

            builder = finish_signer(&mut request, credentials_provider, builder).await?;
        } else {
            if let Some(ce) = self.compression.content_encoding() {
                builder = builder.header("Content-Encoding", ce);
            }

            for (header, value) in &self.request.headers {
                builder = builder.header(&header[..], &value[..]);
            }

            if let Some(auth) = &self.authorization {
                builder = auth.apply_builder(builder);
            }
        }

        let request = builder.body(Body::empty())?;
        let proxy = ProxyConfig::default();
        let client = HttpClient::new(self.tls_settings.clone(), &proxy)
            .expect("Could not build client to flush");
        let response = client.send(request).await?;

        match response.status() {
            StatusCode::OK => Ok(()),
            status => Err(HealthcheckError::UnexpectedStatus { status }.into()),
        }
    }
}

async fn flush(common: ElasticSearchCommon) -> crate::Result<()> {
    use tokio::time::{sleep, Duration};
    sleep(Duration::from_secs(2)).await;
    common.flush_request().await?;
    sleep(Duration::from_secs(2)).await;

    Ok(())
}

async fn create_template_index(common: &ElasticSearchCommon, name: &str) -> crate::Result<()> {
    let client = create_http_client();
    let uri = format!("{}/_index_template/{}", common.base_url, name);
    let response = client
        .put(uri)
        .json(&json!({
            "index_patterns": ["my-*-*"],
            "data_stream": {},
        }))
        .send()
        .await?;
    assert_eq!(response.status(), StatusCode::OK);
    Ok(())
}

#[test]
fn ensure_pipeline_in_params() {
    let index = gen_index();
    let pipeline = String::from("test-pipeline");

    let config = ElasticSearchConfig {
        endpoint: "http://localhost:9200".into(),
        bulk: Some(BulkConfig {
            index: Some(index),
            action: None,
        }),
        pipeline: Some(pipeline.clone()),
        ..config()
    };
    let common = ElasticSearchCommon::parse_config(&config).expect("Config error");

    assert_eq!(common.query_params["pipeline"], pipeline);
}

#[tokio::test]
async fn structures_events_correctly() {
    let index = gen_index();
    let config = ElasticSearchConfig {
        endpoint: http_server(),
        bulk: Some(BulkConfig {
            index: Some(index.clone()),
            action: None,
        }),
        doc_type: Some("log_lines".into()),
        id_key: Some("my_id".into()),
        compression: Compression::None,
        ..config()
    };
    let common = ElasticSearchCommon::parse_config(&config).expect("Config error");
    let base_url = common.base_url.clone();

    let cx = SinkContext::new_test();
    let (sink, _hc) = config.build(cx.clone()).await.unwrap();

    let (batch, mut receiver) = BatchNotifier::new_with_receiver();
    let mut input_event = LogEvent::from("raw log line").with_batch_notifier(&batch);
    input_event.insert("my_id", "42");
    input_event.insert("foo", "bar");
    drop(batch);

    let timestamp = input_event[crate::config::log_schema().timestamp_key()].clone();

    sink.run_events(vec![input_event.into()]).await.unwrap();

    assert_eq!(receiver.try_recv(), Ok(BatchStatus::Delivered));

    // make sure writes all all visible
    flush(common).await.unwrap();

    let response = reqwest::Client::new()
        .get(&format!("{}/{}/_search", base_url, index))
        .json(&json!({
            "query": { "query_string": { "query": "*" } }
        }))
        .send()
        .await
        .unwrap()
        .json::<Value>()
        .await
        .unwrap();

    let total = response["hits"]["total"]
        .as_u64()
        .or_else(|| response["hits"]["total"]["value"].as_u64())
        .expect("Elasticsearch response does not include hits->total nor hits->total->value");
    assert_eq!(1, total);

    let hits = response["hits"]["hits"]
        .as_array()
        .expect("Elasticsearch response does not include hits->hits");

    let hit = hits.iter().next().unwrap();
    assert_eq!("42", hit["_id"]);

    let value = hit
        .get("_source")
        .expect("Elasticsearch hit missing _source");
    assert_eq!(None, value["my_id"].as_str());

    let expected = json!({
        "message": "raw log line",
        "foo": "bar",
        "timestamp": timestamp,
    });
    assert_eq!(&expected, value);
}

#[tokio::test]
async fn insert_events_over_http() {
    trace_init();

    run_insert_tests(
        ElasticSearchConfig {
            endpoint: http_server(),
            doc_type: Some("log_lines".into()),
            compression: Compression::None,
            ..config()
        },
        false,
        BatchStatus::Delivered,
    )
    .await;
}

#[tokio::test]
async fn insert_events_over_https() {
    trace_init();

    run_insert_tests(
        ElasticSearchConfig {
            auth: Some(ElasticSearchAuth::Basic {
                user: "elastic".into(),
                password: "vector".into(),
            }),
            endpoint: https_server(),
            doc_type: Some("log_lines".into()),
            compression: Compression::None,
            tls: Some(TlsOptions {
                ca_file: Some(tls::TEST_PEM_CA_PATH.into()),
                ..Default::default()
            }),
            ..config()
        },
        false,
        BatchStatus::Delivered,
    )
    .await;
}

#[tokio::test]
async fn insert_events_on_aws() {
    trace_init();

    run_insert_tests(
        ElasticSearchConfig {
            auth: Some(ElasticSearchAuth::Aws(AwsAuthentication::Default {})),
            endpoint: aws_server(),
            ..config()
        },
        false,
        BatchStatus::Delivered,
    )
    .await;
}

#[tokio::test]
async fn insert_events_on_aws_with_compression() {
    trace_init();

    run_insert_tests(
        ElasticSearchConfig {
            auth: Some(ElasticSearchAuth::Aws(AwsAuthentication::Default {})),
            endpoint: aws_server(),
            compression: Compression::gzip_default(),
            ..config()
        },
        false,
        BatchStatus::Delivered,
    )
    .await;
}

#[tokio::test]
async fn insert_events_with_failure() {
    trace_init();

    run_insert_tests(
        ElasticSearchConfig {
            endpoint: http_server(),
            doc_type: Some("log_lines".into()),
            compression: Compression::None,
            ..config()
        },
        true,
        BatchStatus::Rejected,
    )
    .await;
}

#[tokio::test]
async fn insert_events_in_data_stream() {
    trace_init();
    let template_index = format!("my-template-{}", gen_index());
    let stream_index = format!("my-stream-{}", gen_index());

    let cfg = ElasticSearchConfig {
        endpoint: http_server(),
        mode: ElasticSearchMode::DataStream,
        bulk: Some(BulkConfig {
            index: Some(stream_index.clone()),
            action: None,
        }),
        ..config()
    };
    let common = ElasticSearchCommon::parse_config(&cfg).expect("Config error");

    create_template_index(&common, &template_index)
        .await
        .expect("Template index creation error");

    create_data_stream(&common, &stream_index)
        .await
        .expect("Data stream creation error");

    run_insert_tests_with_config(&cfg, false, BatchStatus::Delivered).await;
}

async fn run_insert_tests(
    mut config: ElasticSearchConfig,
    break_events: bool,
    status: BatchStatus,
) {
    config.bulk = Some(BulkConfig {
        index: Some(gen_index()),
        action: None,
    });
    run_insert_tests_with_config(&config, break_events, status).await;
}

fn create_http_client() -> reqwest::Client {
    let mut test_ca = Vec::<u8>::new();
    File::open(tls::TEST_PEM_CA_PATH)
        .unwrap()
        .read_to_end(&mut test_ca)
        .unwrap();
    let test_ca = reqwest::Certificate::from_pem(&test_ca).unwrap();

    reqwest::Client::builder()
        .add_root_certificate(test_ca)
        .danger_accept_invalid_certs(true)
        .build()
        .expect("Could not build HTTP client")
}

async fn run_insert_tests_with_config(
    config: &ElasticSearchConfig,
    break_events: bool,
    batch_status: BatchStatus,
) {
    let common = ElasticSearchCommon::parse_config(config).expect("Config error");
    let index = match config.mode {
        // Data stream mode uses an index name generated from the event.
        ElasticSearchMode::DataStream => format!(
            "{}",
            Utc::now().format(".ds-logs-generic-default-%Y.%m.%d-000001")
        ),
        ElasticSearchMode::Bulk => config
            .bulk
            .as_ref()
            .map(|x| x.index.clone().unwrap())
            .unwrap(),
    };
    let base_url = common.base_url.clone();

    let cx = SinkContext::new_test();
    let (sink, healthcheck) = config
        .build(cx.clone())
        .await
        .expect("Building config failed");

    healthcheck.await.expect("Health check failed");

    let (batch, mut receiver) = BatchNotifier::new_with_receiver();
    let (input, events) = random_events_with_stream(100, 100, Some(batch));
    if break_events {
        // Break all but the first event to simulate some kind of partial failure
        let mut doit = false;
        sink.run(events.map(move |mut events| {
            if doit {
                events.for_each_log(|log| {
                    log.insert("_type", 1);
                });
            }
            doit = true;
            events
        }))
        .await
        .expect("Sending events failed");
    } else {
        sink.run(events).await.expect("Sending events failed");
    }

    assert_eq!(receiver.try_recv(), Ok(batch_status));

    // make sure writes all all visible
    flush(common).await.expect("Flushing writes failed");

    let client = create_http_client();
    let mut response = client
        .get(&format!("{}/{}/_search", base_url, index))
        .basic_auth("elastic", Some("vector"))
        .json(&json!({
            "query": { "query_string": { "query": "*" } }
        }))
        .send()
        .await
        .unwrap()
        .json::<Value>()
        .await
        .unwrap();

    let total = response["hits"]["total"]["value"]
        .as_u64()
        .or_else(|| response["hits"]["total"].as_u64())
        .expect("Elasticsearch response does not include hits->total nor hits->total->value");

    if break_events {
        assert_ne!(input.len() as u64, total);
    } else {
        assert_eq!(input.len() as u64, total);

        let hits = response["hits"]["hits"]
            .as_array_mut()
            .expect("Elasticsearch response does not include hits->hits");
        #[allow(clippy::needless_collect)]
        // https://github.com/rust-lang/rust-clippy/issues/6909
        let input = input
            .into_iter()
            .map(|rec| serde_json::to_value(&rec.into_log()).unwrap())
            .collect::<Vec<_>>();

        for hit in hits {
            let hit = hit
                .get_mut("_source")
                .expect("Elasticsearch hit missing _source");
            if config.mode == ElasticSearchMode::DataStream {
                let obj = hit.as_object_mut().unwrap();
                obj.remove("data_stream");
                // Un-rewrite the timestamp field
                let timestamp = obj.remove(DATA_STREAM_TIMESTAMP_KEY).unwrap();
                obj.insert(log_schema().timestamp_key().into(), timestamp);
            }
            assert!(input.contains(hit));
        }
    }
}

fn gen_index() -> String {
    format!("test-{}", random_string(10).to_lowercase())
}

async fn create_data_stream(common: &ElasticSearchCommon, name: &str) -> crate::Result<()> {
    let client = create_http_client();
    let uri = format!("{}/_data_stream/{}", common.base_url, name);
    let response = client
        .put(uri)
        .header("Content-Type", "application/json")
        .send()
        .await?;
    assert_eq!(response.status(), StatusCode::OK);
    Ok(())
}

fn config() -> ElasticSearchConfig {
    let mut batch = BatchConfig::default();
    batch.max_events = Some(1);

    ElasticSearchConfig {
        batch,
        ..Default::default()
    }
}
