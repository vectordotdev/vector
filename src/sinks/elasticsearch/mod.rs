mod retry;
mod config;
mod common;
mod request_builder;
mod encoder;
mod sink;
mod service;

pub use common::*;
pub use config::*;
pub use encoder::Encoding;

use self::retry::{ElasticSearchRetryLogic, ElasticSearchServiceLogic};
use crate::{
    config::{log_schema, DataType, SinkConfig, SinkContext, SinkDescription},
    emit,
    http::{Auth, HttpClient, MaybeAuth},
    internal_events::{ElasticSearchEventEncoded, TemplateRenderingFailed},
    rusoto::{self, region_from_endpoint, AwsAuthentication, RegionOrEndpoint},
    sinks::util::{
        encoding::{EncodingConfigWithDefault, EncodingConfiguration},
        http::{BatchedHttpSink, HttpSink, RequestConfig},
        BatchConfig, BatchSettings, Buffer, Compression, TowerRequestConfig, UriSerde,
    },
    template::{Template, TemplateParseError},
    tls::{TlsOptions, TlsSettings},
    transforms::metric_to_log::{MetricToLog, MetricToLogConfig},
};
use futures::{FutureExt, SinkExt};
use http::{
    header::{HeaderName, HeaderValue},
    uri::InvalidUri,
    Request, StatusCode, Uri,
};
use hyper::Body;
use indexmap::IndexMap;
use rusoto_core::Region;
use rusoto_credential::{CredentialsError, ProvideAwsCredentials};
use rusoto_signature::{SignedRequest, SignedRequestPayload};
use serde::{Deserialize, Serialize};
use serde_json::json;
use snafu::{ResultExt, Snafu};
use std::collections::{BTreeMap, HashMap};
use std::convert::TryFrom;
use vector_core::event::{Event, Value};
use crate::event::{EventRef, LogEvent};
// use crate::sinks::elasticsearch::ParseError::AwsCredentialsGenerateFailed;

#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(deny_unknown_fields, rename_all = "snake_case", tag = "strategy")]
pub enum ElasticSearchAuth {
    Basic { user: String, password: String },
    Aws(AwsAuthentication),
}

#[derive(Deserialize, Serialize, Debug, Clone, Eq, PartialEq)]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub enum ElasticSearchMode {
    Normal,
    DataStream,
}

impl Default for ElasticSearchMode {
    fn default() -> Self {
        Self::Normal
    }
}

#[derive(Derivative, Deserialize, Serialize, Clone, Copy, Debug)]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub enum BulkAction {
    Index,
    Create,
}

#[allow(clippy::trivially_copy_pass_by_ref)]
impl BulkAction {
    pub const fn as_str(&self) -> &'static str {
        match self {
            BulkAction::Index => "index",
            BulkAction::Create => "create",
        }
    }

    pub const fn as_json_pointer(&self) -> &'static str {
        match self {
            BulkAction::Index => "/index",
            BulkAction::Create => "/create",
        }
    }
}

impl TryFrom<&str> for BulkAction {
    type Error = String;

    fn try_from(input: &str) -> Result<Self, Self::Error> {
        match input {
            "index" => Ok(BulkAction::Index),
            "create" => Ok(BulkAction::Create),
            _ => Err(format!("Invalid bulk action: {}", input)),
        }
    }
}

inventory::submit! {
    SinkDescription::new::<ElasticSearchConfig>("elasticsearch")
}

impl_generate_config_from_default!(ElasticSearchConfig);

// <<<<<<< HEAD
// =======
// #[async_trait::async_trait]
// #[typetag::serde(name = "elasticsearch")]
// impl SinkConfig for ElasticSearchConfig {
//     async fn build(
//         &self,
//         cx: SinkContext,
//     ) -> crate::Result<(super::VectorSink, super::Healthcheck)> {
//         let common = ElasticSearchCommon::parse_config(self)?;
//         let client = HttpClient::new(common.tls_settings.clone(), cx.proxy())?;
//
//         let healthcheck = common.healthcheck(client.clone()).boxed();
//
//         let common = ElasticSearchCommon::parse_config(self)?;
//         let compression = common.compression;
//         let batch = BatchSettings::default()
//             .bytes(10_000_000)
//             .timeout(1)
//             .parse_config(self.batch)?;
//         let request = self
//             .request
//             .tower
//             .unwrap_with(&TowerRequestConfig::default());
//
//         let sink = BatchedHttpSink::with_logic(
//             common,
//             Buffer::new(batch.size, compression),
//             ElasticSearchRetryLogic,
//             request,
//             batch.timeout,
//             client,
//             cx.acker(),
//             ElasticSearchServiceLogic,
//         )
//         .sink_map_err(|error| error!(message = "Fatal elasticsearch sink error.", %error));
//
//         Ok((super::VectorSink::Sink(Box::new(sink)), healthcheck))
//     }
//
//     fn input_type(&self) -> DataType {
//         DataType::Any
//     }
// >>>>>>> master


#[derive(Debug)]
pub enum ElasticSearchCommonMode {
    Normal {
        index: Template,
        bulk_action: Option<Template>,
    },
    DataStream(DataStreamConfig),
}

impl ElasticSearchCommonMode {
    fn index<'a>(&self, event: impl Into<EventRef<'a>>) -> Option<String> {
        match self {
            Self::Normal { index, .. } => index
                .render_string(log)
                .map_err(|error| {
                    emit!(&TemplateRenderingFailed {
                        error,
                        field: Some("index"),
                        drop_event: true,
                    });
                })
                .ok(),
            Self::DataStream(ds) => ds.index(log),
        }
    }

    fn bulk_action<'a>(&self, event: impl Into<EventRef<'a>>) -> Option<BulkAction> {
        match self {
            ElasticSearchCommonMode::Normal { bulk_action, .. } => match bulk_action {
                Some(template) => template
                    .render_string(event)
                    .map_err(|error| {
                        emit!(&TemplateRenderingFailed {
                            error,
                            field: Some("bulk_action"),
                            drop_event: true,
                        });
                    })
                    .ok()
                    .and_then(|value| BulkAction::try_from(value.as_str()).ok()),
                None => Some(BulkAction::Index),
            },
            // avoid the interpolation
            ElasticSearchCommonMode::DataStream(_) => Some(BulkAction::Create),
        }
    }

    const fn as_data_stream_config(&self) -> Option<&DataStreamConfig> {
        match self {
            Self::DataStream(value) => Some(value),
            _ => None,
        }
    }
}



#[derive(Debug, Snafu)]
#[snafu(visibility(pub))]
pub enum ParseError {
    #[snafu(display("Invalid host {:?}: {:?}", host, source))]
    InvalidHost { host: String, source: InvalidUri },
    #[snafu(display("Host {:?} must include hostname", host))]
    HostMustIncludeHostname { host: String },
    #[snafu(display("Could not generate AWS credentials: {:?}", source))]
    AwsCredentialsGenerateFailed { source: CredentialsError },
    #[snafu(display("Index template parse error: {}", source))]
    IndexTemplate { source: TemplateParseError },
    #[snafu(display("Batch action template parse error: {}", source))]
    BatchActionTemplate { source: TemplateParseError },
}





async fn finish_signer(
    signer: &mut SignedRequest,
    credentials_provider: &rusoto::AwsCredentialsProvider,
    mut builder: http::request::Builder,
) -> crate::Result<http::request::Builder> {
    let credentials = credentials_provider
        .credentials()
        .await
        .context(AwsCredentialsGenerateFailed)?;

    signer.sign(&credentials);

    for (name, values) in signer.headers() {
        let header_name = name
            .parse::<HeaderName>()
            .expect("Could not parse header name.");
        for value in values {
            let header_value =
                HeaderValue::from_bytes(value).expect("Could not parse header value.");
            builder = builder.header(&header_name, header_value);
        }
    }

    Ok(builder)
}

fn maybe_set_id(key: Option<impl AsRef<str>>, doc: &mut serde_json::Value, log: &mut LogEvent) {
    if let Some(val) = key.and_then(|k| log.remove(k)) {
        let val = val.to_string_lossy();

        doc.as_object_mut()
            .unwrap()
            .insert("_id".into(), json!(val));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        event::{Event, Metric, MetricKind, MetricValue, Value},
        sinks::util::retries::{RetryAction, RetryLogic},
    };
    use bytes::Bytes;
    use http::{Response, StatusCode};
    use pretty_assertions::assert_eq;
    use serde_json::json;
    use std::collections::BTreeMap;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<ElasticSearchConfig>();
    }

    #[test]
    fn parse_aws_auth() {
        toml::from_str::<ElasticSearchConfig>(
            r#"
            endpoint = ""
            auth.strategy = "aws"
            auth.assume_role = "role"
        "#,
        )
        .unwrap();

        toml::from_str::<ElasticSearchConfig>(
            r#"
            endpoint = ""
            auth.strategy = "aws"
        "#,
        )
        .unwrap();
    }

    #[test]
    fn parse_mode() {
        let config = toml::from_str::<ElasticSearchConfig>(
            r#"
            endpoint = ""
            mode = "data_stream"
            data_stream.type = "synthetics"
        "#,
        )
        .unwrap();
        assert!(matches!(config.mode, ElasticSearchMode::DataStream));
        assert!(config.data_stream.is_some());
    }

    #[test]
    fn removes_and_sets_id_from_custom_field() {
        let id_key = Some("foo");
        let mut log = LogEvent::from("butts");
        log.as_mut_log().insert("foo", "bar");
        let mut action = json!({});

        maybe_set_id(id_key, &mut action, &mut log);

        assert_eq!(json!({"_id": "bar"}), action);
        assert_eq!(None, event.as_log().get("foo"));
    }

    #[test]
    fn doesnt_set_id_when_field_missing() {
        let id_key = Some("foo");
        let mut event = Event::from("butts");
        event.as_mut_log().insert("not_foo", "bar");
        let mut action = json!({});

        maybe_set_id(id_key, &mut action, &mut event);

        assert_eq!(json!({}), action);
    }

    #[test]
    fn doesnt_set_id_when_not_configured() {
        let id_key: Option<&str> = None;
        let mut event = Event::from("butts");
        event.as_mut_log().insert("foo", "bar");
        let mut action = json!({});

        maybe_set_id(id_key, &mut action, &mut event);

        assert_eq!(json!({}), action);
    }

    #[test]
    fn sets_create_action_when_configured() {
        use crate::config::log_schema;
        use chrono::{TimeZone, Utc};

        let config = ElasticSearchConfig {
            bulk_action: Some(String::from("{{ action }}te")),
            index: Some(String::from("vector")),
            endpoint: String::from("https://example.com"),
            ..Default::default()
        };
        let es = ElasticSearchCommon::parse_config(&config).unwrap();

        let mut event = Event::from("hello there");
        event.as_mut_log().insert(
            log_schema().timestamp_key(),
            Utc.ymd(2020, 12, 1).and_hms(1, 2, 3),
        );
        event.as_mut_log().insert("action", "crea");
        let encoded = es.encode_event(event).unwrap();
        let expected = r#"{"create":{"_index":"vector","_type":"_doc"}}
{"action":"crea","message":"hello there","timestamp":"2020-12-01T01:02:03Z"}
"#;
        assert_eq!(std::str::from_utf8(&encoded).unwrap(), expected);
    }

    fn data_stream_body() -> BTreeMap<String, Value> {
        let mut ds = BTreeMap::<String, Value>::new();
        ds.insert("type".into(), Value::from("synthetics"));
        ds.insert("dataset".into(), Value::from("testing"));
        ds
    }

    #[test]
    fn encode_datastream_mode() {
        use crate::config::log_schema;
        use chrono::{TimeZone, Utc};

        let config = ElasticSearchConfig {
            index: Some(String::from("vector")),
            endpoint: String::from("https://example.com"),
            mode: ElasticSearchMode::DataStream,
            ..Default::default()
        };
        let es = ElasticSearchCommon::parse_config(&config).unwrap();

        let mut event = Event::from("hello there");
        event.as_mut_log().insert(
            log_schema().timestamp_key(),
            Utc.ymd(2020, 12, 1).and_hms(1, 2, 3),
        );
        event.as_mut_log().insert("data_stream", data_stream_body());
        let encoded = es.encode_event(event).unwrap();
        let expected = r#"{"create":{"_index":"synthetics-testing-default","_type":"_doc"}}
{"@timestamp":"2020-12-01T01:02:03Z","data_stream":{"dataset":"testing","namespace":"default","type":"synthetics"},"message":"hello there"}
"#;
        assert_eq!(std::str::from_utf8(&encoded).unwrap(), expected);
    }

    #[test]
    fn encode_datastream_mode_no_routing() {
        use crate::config::log_schema;
        use chrono::{TimeZone, Utc};

        let config = ElasticSearchConfig {
            index: Some(String::from("vector")),
            endpoint: String::from("https://example.com"),
            mode: ElasticSearchMode::DataStream,
            data_stream: Some(DataStreamConfig {
                auto_routing: false,
                namespace: Template::try_from("something").unwrap(),
                ..Default::default()
            }),
            ..Default::default()
        };
        let es = ElasticSearchCommon::parse_config(&config).unwrap();

        let mut event = Event::from("hello there");
        event.as_mut_log().insert("data_stream", data_stream_body());
        event.as_mut_log().insert(
            log_schema().timestamp_key(),
            Utc.ymd(2020, 12, 1).and_hms(1, 2, 3),
        );
        let encoded = es.encode_event(event).unwrap();
        let expected = r#"{"create":{"_index":"logs-generic-something","_type":"_doc"}}
{"@timestamp":"2020-12-01T01:02:03Z","data_stream":{"dataset":"testing","namespace":"something","type":"synthetics"},"message":"hello there"}
"#;
        assert_eq!(std::str::from_utf8(&encoded).unwrap(), expected);
    }

    #[test]
    fn handle_metrics() {
        let config = ElasticSearchConfig {
            bulk_action: Some(String::from("create")),
            index: Some(String::from("vector")),
            endpoint: String::from("https://example.com"),
            ..Default::default()
        };
        let es = ElasticSearchCommon::parse_config(&config).unwrap();

        let metric = Metric::new(
            "cpu",
            MetricKind::Absolute,
            MetricValue::Gauge { value: 42.0 },
        );
        let event = Event::from(metric);

        let encoded = es.encode_event(event).unwrap();
        let encoded = std::str::from_utf8(&encoded).unwrap();
        let encoded_lines = encoded.split('\n').map(String::from).collect::<Vec<_>>();
        assert_eq!(encoded_lines.len(), 3); // there's an empty line at the end
        assert_eq!(
            encoded_lines.get(0).unwrap(),
            r#"{"create":{"_index":"vector","_type":"_doc"}}"#
        );
        assert!(encoded_lines
            .get(1)
            .unwrap()
            .starts_with(r#"{"gauge":{"value":42.0},"kind":"absolute","name":"cpu","timestamp""#));
    }

    #[test]
    fn decode_bulk_action_error() {
        let config = ElasticSearchConfig {
            bulk_action: Some(String::from("{{ action }}")),
            index: Some(String::from("vector")),
            endpoint: String::from("https://example.com"),
            ..Default::default()
        };
        let es = ElasticSearchCommon::parse_config(&config).unwrap();

        let mut event = Event::from("hello world");
        event.as_mut_log().insert("foo", "bar");
        event.as_mut_log().insert("idx", "purple");
        let action = es.mode.bulk_action(&event);
        assert!(action.is_none());
    }

    #[test]
    fn decode_bulk_action() {
        let config = ElasticSearchConfig {
            bulk_action: Some(String::from("create")),
            index: Some(String::from("vector")),
            endpoint: String::from("https://example.com"),
            ..Default::default()
        };
        let es = ElasticSearchCommon::parse_config(&config).unwrap();

        let event = Event::from("hello there");
        let action = es.mode.bulk_action(&event).unwrap();
        assert!(matches!(action, BulkAction::Create));
    }

    #[test]
    fn encode_datastream_mode_no_sync() {
        use crate::config::log_schema;
        use chrono::{TimeZone, Utc};

        let config = ElasticSearchConfig {
            index: Some(String::from("vector")),
            endpoint: String::from("https://example.com"),
            mode: ElasticSearchMode::DataStream,
            data_stream: Some(DataStreamConfig {
                namespace: Template::try_from("something").unwrap(),
                sync_fields: false,
                ..Default::default()
            }),
            ..Default::default()
        };

        let es = ElasticSearchCommon::parse_config(&config).unwrap();

        let mut event = Event::from("hello there");
        event.as_mut_log().insert("data_stream", data_stream_body());
        event.as_mut_log().insert(
            log_schema().timestamp_key(),
            Utc.ymd(2020, 12, 1).and_hms(1, 2, 3),
        );
        let encoded = es.encode_event(event).unwrap();
        let expected = r#"{"create":{"_index":"synthetics-testing-something","_type":"_doc"}}
{"@timestamp":"2020-12-01T01:02:03Z","data_stream":{"dataset":"testing","type":"synthetics"},"message":"hello there"}
"#;
        assert_eq!(std::str::from_utf8(&encoded).unwrap(), expected);
    }

    #[test]
    fn handles_error_response() {
        let json = "{\"took\":185,\"errors\":true,\"items\":[{\"index\":{\"_index\":\"test-hgw28jv10u\",\"_type\":\"log_lines\",\"_id\":\"3GhQLXEBE62DvOOUKdFH\",\"status\":400,\"error\":{\"type\":\"illegal_argument_exception\",\"reason\":\"mapper [message] of different type, current_type [long], merged_type [text]\"}}}]}";
        let response = Response::builder()
            .status(StatusCode::OK)
            .body(Bytes::from(json))
            .unwrap();
        let logic = ElasticSearchRetryLogic;
        assert!(matches!(
            logic.should_retry_response(&response),
            RetryAction::DontRetry(_)
        ));
    }

    #[test]
    fn allows_using_excepted_fields() {
        let config = ElasticSearchConfig {
            index: Some(String::from("{{ idx }}")),
            encoding: EncodingConfigWithDefault {
                except_fields: Some(vec!["idx".to_string(), "timestamp".to_string()]),
                ..Default::default()
            },
            endpoint: String::from("https://example.com"),
            ..Default::default()
        };
        let es = ElasticSearchCommon::parse_config(&config).unwrap();

        let mut event = Event::from("hello there");
        event.as_mut_log().insert("foo", "bar");
        event.as_mut_log().insert("idx", "purple");

        let encoded = es.encode_event(event).unwrap();
        let expected = r#"{"index":{"_index":"purple","_type":"_doc"}}
{"foo":"bar","message":"hello there"}
"#;
        assert_eq!(std::str::from_utf8(&encoded).unwrap(), expected);
    }

    #[test]
    fn validate_host_header_on_aws_requests() {
        let config = ElasticSearchConfig {
            auth: Some(ElasticSearchAuth::Aws(AwsAuthentication::Default {})),
            endpoint: "http://abc-123.us-east-1.es.amazonaws.com".into(),
            batch: BatchConfig {
                max_events: Some(1),
                ..Default::default()
            },
            ..Default::default()
        };

        let common = ElasticSearchCommon::parse_config(&config).expect("Config error");

        let signed_request = common.signed_request(
            "POST",
            &"http://abc-123.us-east-1.es.amazonaws.com"
                .parse::<Uri>()
                .unwrap(),
            true,
        );

        assert_eq!(
            signed_request.hostname(),
            "abc-123.us-east-1.es.amazonaws.com".to_string()
        );
    }
}

#[cfg(test)]
#[cfg(feature = "es-integration-tests")]
mod integration_tests {
    use super::*;
    use crate::{
        config::{ProxyConfig, SinkConfig, SinkContext},
        http::HttpClient,
        sinks::HealthcheckError,
        test_util::{random_events_with_stream, random_string, trace_init},
        tls::{self, TlsOptions},
    };
    use chrono::Utc;
    use futures::{stream, StreamExt};
    use http::{Request, StatusCode};
    use hyper::Body;
    use serde_json::{json, Value};
    use std::{fs::File, future::ready, io::Read};
    use vector_core::event::{BatchNotifier, BatchStatus, LogEvent};

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
            index: Some(index),
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
            endpoint: "http://localhost:9200".into(),
            index: Some(index.clone()),
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

        sink.run(stream::once(ready(input_event.into())))
            .await
            .unwrap();

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
                endpoint: "http://localhost:9200".into(),
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
                endpoint: "https://localhost:9201".into(),
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
                endpoint: "http://localhost:4571".into(),
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
                endpoint: "http://localhost:4571".into(),
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
                endpoint: "http://localhost:9200".into(),
                doc_type: Some("log_lines".into()),
                compression: Compression::None,
                ..config()
            },
            true,
            BatchStatus::Failed,
        )
        .await;
    }

    #[tokio::test]
    async fn insert_events_in_data_stream() {
        trace_init();
        let template_index = format!("my-template-{}", gen_index());
        let stream_index = format!("my-stream-{}", gen_index());

        let cfg = ElasticSearchConfig {
            endpoint: "http://localhost:9200".into(),
            mode: ElasticSearchMode::DataStream,
            index: Some(stream_index.clone()),
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
        config.index = Some(gen_index());
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
            ElasticSearchMode::Normal => config.index.clone().unwrap(),
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
            sink.run(events.map(move |mut event| {
                if doit {
                    event.as_mut_log().insert("_type", 1);
                }
                doit = true;
                event
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
        ElasticSearchConfig {
            batch: BatchConfig {
                max_events: Some(1),
                ..Default::default()
            },
            ..Default::default()
        }
    }
}
