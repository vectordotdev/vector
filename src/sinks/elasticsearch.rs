use crate::{
    config::{DataType, SinkConfig, SinkContext, SinkDescription},
    emit,
    event::Event,
    http::{Auth, HttpClient, HttpError, MaybeAuth},
    internal_events::{ElasticSearchEventEncoded, ElasticSearchMissingKeys},
    rusoto::{self, region_from_endpoint, AWSAuthentication, RegionOrEndpoint},
    sinks::util::{
        encoding::{EncodingConfigWithDefault, EncodingConfiguration},
        http::{BatchedHttpSink, HttpSink, RequestConfig},
        retries::{RetryAction, RetryLogic},
        BatchConfig, BatchSettings, Buffer, Compression, TowerRequestConfig, UriSerde,
    },
    template::{Template, TemplateError},
    tls::{TlsOptions, TlsSettings},
};
use bytes::Bytes;
use futures::{FutureExt, SinkExt};
use http::{
    header::{HeaderName, HeaderValue},
    uri::InvalidUri,
    Request, StatusCode, Uri,
};
use hyper::Body;
use indexmap::IndexMap;
use lazy_static::lazy_static;
use rusoto_core::Region;
use rusoto_credential::{CredentialsError, ProvideAwsCredentials};
use rusoto_signature::{SignedRequest, SignedRequestPayload};
use serde::{Deserialize, Serialize};
use serde_json::json;
use snafu::{ResultExt, Snafu};
use std::collections::HashMap;
use std::convert::TryFrom;

#[derive(Deserialize, Serialize, Debug, Clone, Default)]
#[serde(deny_unknown_fields)]
pub struct ElasticSearchConfig {
    // Deprecated name
    #[serde(alias = "host")]
    pub endpoint: String,
    pub index: Option<String>,
    pub doc_type: Option<String>,
    pub id_key: Option<String>,
    pub pipeline: Option<String>,

    #[serde(default)]
    pub compression: Compression,
    #[serde(
        skip_serializing_if = "crate::serde::skip_serializing_if_default",
        default
    )]
    pub encoding: EncodingConfigWithDefault<Encoding>,
    #[serde(default)]
    pub batch: BatchConfig,
    #[serde(default)]
    pub request: RequestConfig,
    pub auth: Option<ElasticSearchAuth>,

    // Deprecated, moved to request.
    pub headers: Option<IndexMap<String, String>>,
    pub query: Option<HashMap<String, String>>,

    pub aws: Option<RegionOrEndpoint>,
    pub tls: Option<TlsOptions>,
    #[serde(default)]
    pub bulk_action: BulkAction,
}

lazy_static! {
    static ref REQUEST_DEFAULTS: TowerRequestConfig = TowerRequestConfig {
        ..Default::default()
    };
}

#[derive(Deserialize, Serialize, Debug, Eq, PartialEq, Clone, Derivative)]
#[serde(rename_all = "snake_case")]
#[derivative(Default)]
pub enum Encoding {
    #[derivative(Default)]
    Default,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(deny_unknown_fields, rename_all = "snake_case", tag = "strategy")]
pub enum ElasticSearchAuth {
    Basic { user: String, password: String },
    Aws(AWSAuthentication),
}

#[derive(Derivative, Deserialize, Serialize, Clone, Debug)]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
#[derivative(Default)]
pub enum BulkAction {
    #[derivative(Default)]
    Index,
    Create,
}

impl BulkAction {
    pub fn as_str(&self) -> &'static str {
        match *self {
            BulkAction::Index => "index",
            BulkAction::Create => "create",
        }
    }

    pub fn as_json_pointer(&self) -> &'static str {
        match *self {
            BulkAction::Index => "/index",
            BulkAction::Create => "/create",
        }
    }
}

inventory::submit! {
    SinkDescription::new::<ElasticSearchConfig>("elasticsearch")
}

impl_generate_config_from_default!(ElasticSearchConfig);

#[async_trait::async_trait]
#[typetag::serde(name = "elasticsearch")]
impl SinkConfig for ElasticSearchConfig {
    async fn build(
        &self,
        cx: SinkContext,
    ) -> crate::Result<(super::VectorSink, super::Healthcheck)> {
        let common = ElasticSearchCommon::parse_config(&self)?;
        let client = HttpClient::new(common.tls_settings.clone())?;

        let healthcheck = healthcheck(client.clone(), common).boxed();

        let common = ElasticSearchCommon::parse_config(&self)?;
        let compression = common.compression;
        let batch = BatchSettings::default()
            .bytes(bytesize::mib(10u64))
            .timeout(1)
            .parse_config(self.batch)?;
        let request = self.request.tower.unwrap_with(&REQUEST_DEFAULTS);

        let sink = BatchedHttpSink::with_retry_logic(
            common,
            Buffer::new(batch.size, compression),
            ElasticSearchRetryLogic,
            request,
            batch.timeout,
            client,
            cx.acker(),
        )
        .sink_map_err(|error| error!(message = "Fatal elasticsearch sink error.", %error));

        Ok((super::VectorSink::Sink(Box::new(sink)), healthcheck))
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }

    fn sink_type(&self) -> &'static str {
        "elasticsearch"
    }
}

#[derive(Debug)]
pub struct ElasticSearchCommon {
    pub base_url: String,
    bulk_uri: Uri,
    authorization: Option<Auth>,
    credentials: Option<rusoto::AwsCredentialsProvider>,
    index: Template,
    doc_type: String,
    tls_settings: TlsSettings,
    config: ElasticSearchConfig,
    compression: Compression,
    region: Region,
    query_params: HashMap<String, String>,
    bulk_action: BulkAction,
}

#[derive(Debug, Snafu)]
enum ParseError {
    #[snafu(display("Invalid host {:?}: {:?}", host, source))]
    InvalidHost { host: String, source: InvalidUri },
    #[snafu(display("Host {:?} must include hostname", host))]
    HostMustIncludeHostname { host: String },
    #[snafu(display("Could not generate AWS credentials: {:?}", source))]
    AWSCredentialsGenerateFailed { source: CredentialsError },
    #[snafu(display("Index template parse error: {}", source))]
    IndexTemplate { source: TemplateError },
}

#[async_trait::async_trait]
impl HttpSink for ElasticSearchCommon {
    type Input = Vec<u8>;
    type Output = Vec<u8>;

    fn encode_event(&self, mut event: Event) -> Option<Self::Input> {
        let index = self
            .index
            .render_string(&event)
            .map_err(|missing_keys| {
                emit!(ElasticSearchMissingKeys {
                    keys: &missing_keys
                });
            })
            .ok()?;

        let mut action = json!({
            self.bulk_action.as_str(): {
                "_index": index,
                "_type": self.doc_type,
            }
        });
        maybe_set_id(
            self.config.id_key.as_ref(),
            action
                .pointer_mut(self.bulk_action.as_json_pointer())
                .unwrap(),
            &mut event,
        );

        let mut body = serde_json::to_vec(&action).unwrap();
        body.push(b'\n');

        self.config.encoding.apply_rules(&mut event);

        serde_json::to_writer(&mut body, &event.into_log()).unwrap();
        body.push(b'\n');

        emit!(ElasticSearchEventEncoded {
            byte_size: body.len(),
            index
        });

        Some(body)
    }

    async fn build_request(&self, events: Self::Output) -> crate::Result<http::Request<Vec<u8>>> {
        let mut builder = Request::post(&self.bulk_uri);

        if let Some(credentials_provider) = &self.credentials {
            let mut request = self.signed_request("POST", &self.bulk_uri, true);

            request.add_header("Content-Type", "application/x-ndjson");

            if let Some(ce) = self.compression.content_encoding() {
                request.add_header("Content-Encoding", ce);
            }

            for (header, value) in &self.config.request.headers {
                request.add_header(header, value);
            }

            request.set_payload(Some(events));

            // mut builder?
            builder = finish_signer(&mut request, &credentials_provider, builder).await?;

            // The SignedRequest ends up owning the body, so we have
            // to play games here
            let body = request.payload.take().unwrap();
            match body {
                SignedRequestPayload::Buffer(body) => {
                    builder.body(body.to_vec()).map_err(Into::into)
                }
                _ => unreachable!(),
            }
        } else {
            builder = builder.header("Content-Type", "application/x-ndjson");

            if let Some(ce) = self.compression.content_encoding() {
                builder = builder.header("Content-Encoding", ce);
            }

            for (header, value) in &self.config.request.headers {
                builder = builder.header(&header[..], &value[..]);
            }

            if let Some(auth) = &self.authorization {
                builder = auth.apply_builder(builder);
            }

            builder.body(events).map_err(Into::into)
        }
    }
}

#[derive(Clone)]
struct ElasticSearchRetryLogic;

#[derive(Deserialize, Debug)]
struct ESResultResponse {
    items: Vec<ESResultItem>,
}
#[derive(Deserialize, Debug)]
struct ESResultItem {
    index: ESIndexResult,
}
#[derive(Deserialize, Debug)]
struct ESIndexResult {
    error: Option<ESErrorDetails>,
}
#[derive(Deserialize, Debug)]
struct ESErrorDetails {
    reason: String,
    #[serde(rename = "type")]
    err_type: String,
}

impl RetryLogic for ElasticSearchRetryLogic {
    type Error = HttpError;
    type Response = hyper::Response<Bytes>;

    fn is_retriable_error(&self, _error: &Self::Error) -> bool {
        true
    }

    fn should_retry_response(&self, response: &Self::Response) -> RetryAction {
        let status = response.status();

        match status {
            StatusCode::TOO_MANY_REQUESTS => RetryAction::Retry("too many requests".into()),
            StatusCode::NOT_IMPLEMENTED => {
                RetryAction::DontRetry("endpoint not implemented".into())
            }
            _ if status.is_server_error() => RetryAction::Retry(format!(
                "{}: {}",
                status,
                String::from_utf8_lossy(response.body())
            )),
            _ if status.is_client_error() => {
                let body = String::from_utf8_lossy(response.body());
                RetryAction::DontRetry(format!("client-side error, {}: {}", status, body))
            }
            _ if status.is_success() => {
                let body = String::from_utf8_lossy(response.body());

                if body.contains("\"errors\":true") {
                    RetryAction::DontRetry(get_error_reason(&body))
                } else {
                    RetryAction::Successful
                }
            }
            _ => RetryAction::DontRetry(format!("response status: {}", status)),
        }
    }
}

fn get_error_reason(body: &str) -> String {
    match serde_json::from_str::<ESResultResponse>(&body) {
        Err(json_error) => format!(
            "some messages failed, could not parse response, error: {}",
            json_error
        ),
        Ok(resp) => match resp.items.into_iter().find_map(|item| item.index.error) {
            Some(error) => format!("error type: {}, reason: {}", error.err_type, error.reason),
            None => format!("error response: {}", body),
        },
    }
}

impl ElasticSearchCommon {
    pub fn parse_config(config: &ElasticSearchConfig) -> crate::Result<Self> {
        // Test the configured host, but ignore the result
        let uri = format!("{}/_test", &config.endpoint);
        let uri = uri.parse::<Uri>().with_context(|| InvalidHost {
            host: &config.endpoint,
        })?;
        if uri.host().is_none() {
            return Err(ParseError::HostMustIncludeHostname {
                host: config.endpoint.clone(),
            }
            .into());
        }

        let authorization = match &config.auth {
            Some(ElasticSearchAuth::Basic { user, password }) => Some(Auth::Basic {
                user: user.clone(),
                password: password.clone(),
            }),
            _ => None,
        };
        let uri = config.endpoint.parse::<UriSerde>()?;
        let authorization = authorization.choose_one(&uri.auth)?;
        let base_url = uri.uri.to_string().trim_end_matches('/').to_owned();

        let region = match &config.aws {
            Some(region) => Region::try_from(region)?,
            None => region_from_endpoint(&base_url)?,
        };

        let credentials = match &config.auth {
            Some(ElasticSearchAuth::Basic { .. }) | None => None,
            Some(ElasticSearchAuth::Aws(aws)) => Some(aws.build(&region, None)?),
        };

        let compression = config.compression;
        let index = config.index.as_deref().unwrap_or("vector-%Y.%m.%d");
        let index = Template::try_from(index).context(IndexTemplate)?;

        let doc_type = config.doc_type.clone().unwrap_or_else(|| "_doc".into());
        let bulk_action = config.bulk_action.clone();

        let request = config.request.tower.unwrap_with(&REQUEST_DEFAULTS);

        let mut query_params = config.query.clone().unwrap_or_default();
        query_params.insert("timeout".into(), format!("{}s", request.timeout.as_secs()));

        if let Some(pipeline) = &config.pipeline {
            query_params.insert("pipeline".into(), pipeline.into());
        }

        let mut query = url::form_urlencoded::Serializer::new(String::new());
        for (p, v) in &query_params {
            query.append_pair(&p[..], &v[..]);
        }
        let bulk_url = format!("{}/_bulk?{}", base_url, query.finish());
        let bulk_uri = bulk_url.parse::<Uri>().unwrap();

        let tls_settings = TlsSettings::from_options(&config.tls)?;
        let mut config = config.clone();

        config.request.add_old_option(config.headers.take());

        Ok(Self {
            base_url,
            bulk_uri,
            authorization,
            credentials,
            index,
            doc_type,
            tls_settings,
            config,
            compression,
            region,
            query_params,
            bulk_action,
        })
    }

    fn signed_request(&self, method: &str, uri: &Uri, use_params: bool) -> SignedRequest {
        let mut request = SignedRequest::new(method, "es", &self.region, uri.path());
        if use_params {
            for (key, value) in &self.query_params {
                request.add_param(key, value);
            }
        }
        request
    }
}

async fn healthcheck(client: HttpClient, common: ElasticSearchCommon) -> crate::Result<()> {
    let mut builder = Request::get(format!("{}/_cluster/health", common.base_url));

    match &common.credentials {
        None => {
            if let Some(authorization) = &common.authorization {
                builder = authorization.apply_builder(builder);
            }
        }
        Some(credentials_provider) => {
            let mut signer = common.signed_request("GET", builder.uri_ref().unwrap(), false);
            builder = finish_signer(&mut signer, &credentials_provider, builder).await?;
        }
    }
    let request = builder.body(Body::empty())?;
    let response = client.send(request).await?;

    match response.status() {
        StatusCode::OK => Ok(()),
        status => Err(super::HealthcheckError::UnexpectedStatus { status }.into()),
    }
}

async fn finish_signer(
    signer: &mut SignedRequest,
    credentials_provider: &rusoto::AwsCredentialsProvider,
    mut builder: http::request::Builder,
) -> crate::Result<http::request::Builder> {
    let credentials = credentials_provider
        .credentials()
        .await
        .context(AWSCredentialsGenerateFailed)?;

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

fn maybe_set_id(key: Option<impl AsRef<str>>, doc: &mut serde_json::Value, event: &mut Event) {
    if let Some(val) = key.and_then(|k| event.as_mut_log().remove(k)) {
        let val = val.to_string_lossy();

        doc.as_object_mut()
            .unwrap()
            .insert("_id".into(), json!(val));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{sinks::util::retries::RetryAction, Event};
    use http::{Response, StatusCode};
    use pretty_assertions::assert_eq;
    use serde_json::json;

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
    fn removes_and_sets_id_from_custom_field() {
        let id_key = Some("foo");
        let mut event = Event::from("butts");
        event.as_mut_log().insert("foo", "bar");
        let mut action = json!({});

        maybe_set_id(id_key, &mut action, &mut event);

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
            bulk_action: BulkAction::Create,
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
        let encoded = es.encode_event(event).unwrap();
        let expected = r#"{"create":{"_index":"vector","_type":"_doc"}}
{"message":"hello there","timestamp":"2020-12-01T01:02:03Z"}
"#;
        assert_eq!(std::str::from_utf8(&encoded).unwrap(), &expected[..]);
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
        assert_eq!(std::str::from_utf8(&encoded).unwrap(), &expected[..]);
    }
}

#[cfg(test)]
#[cfg(feature = "es-integration-tests")]
mod integration_tests {
    use super::*;
    use crate::{
        config::{SinkConfig, SinkContext},
        http::HttpClient,
        test_util::{random_events_with_stream, random_string, trace_init},
        tls::{self, TlsOptions},
        Event,
    };
    use futures::{stream, StreamExt};
    use http::{Request, StatusCode};
    use hyper::Body;
    use serde_json::{json, Value};
    use std::{fs::File, future::ready, io::Read};

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

        let mut input_event = Event::from("raw log line");
        input_event.as_mut_log().insert("my_id", "42");
        input_event.as_mut_log().insert("foo", "bar");

        sink.run(stream::once(ready(input_event.clone())))
            .await
            .unwrap();

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
            .expect("Elasticsearch response does not include hits->total");
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
            "timestamp": input_event.as_log()[crate::config::log_schema().timestamp_key()],
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
        )
        .await;
    }

    #[tokio::test]
    async fn insert_events_over_https() {
        trace_init();

        run_insert_tests(
            ElasticSearchConfig {
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
        )
        .await;
    }

    #[tokio::test]
    async fn insert_events_on_aws() {
        trace_init();

        run_insert_tests(
            ElasticSearchConfig {
                auth: Some(ElasticSearchAuth::Aws(AWSAuthentication::Default {})),
                endpoint: "http://localhost:4571".into(),
                ..config()
            },
            false,
        )
        .await;
    }

    #[tokio::test]
    async fn insert_events_on_aws_with_compression() {
        trace_init();

        run_insert_tests(
            ElasticSearchConfig {
                auth: Some(ElasticSearchAuth::Aws(AWSAuthentication::Default {})),
                endpoint: "http://localhost:4571".into(),
                compression: Compression::gzip_default(),
                ..config()
            },
            false,
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
        )
        .await;
    }

    async fn run_insert_tests(mut config: ElasticSearchConfig, break_events: bool) {
        let index = gen_index();
        config.index = Some(index.clone());
        let common = ElasticSearchCommon::parse_config(&config).expect("Config error");
        let base_url = common.base_url.clone();

        let cx = SinkContext::new_test();
        let (sink, healthcheck) = config
            .build(cx.clone())
            .await
            .expect("Building config failed");

        healthcheck.await.expect("Health check failed");

        let (input, events) = random_events_with_stream(100, 100);
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

        // make sure writes all all visible
        flush(common).await.expect("Flushing writes failed");

        let mut test_ca = Vec::<u8>::new();
        File::open(tls::TEST_PEM_CA_PATH)
            .unwrap()
            .read_to_end(&mut test_ca)
            .unwrap();
        let test_ca = reqwest::Certificate::from_pem(&test_ca).unwrap();

        let client = reqwest::Client::builder()
            .add_root_certificate(test_ca)
            .build()
            .expect("Could not build HTTP client");

        let response = client
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
            .expect("Elasticsearch response does not include hits->total");

        if break_events {
            assert_ne!(input.len() as u64, total);
        } else {
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
    }

    fn gen_index() -> String {
        format!("test-{}", random_string(10).to_lowercase())
    }

    async fn flush(common: ElasticSearchCommon) -> crate::Result<()> {
        let uri = format!("{}/_flush", common.base_url);
        let request = Request::post(uri).body(Body::empty()).unwrap();

        let client =
            HttpClient::new(common.tls_settings.clone()).expect("Could not build client to flush");
        let response = client.send(request).await?;
        match response.status() {
            StatusCode::OK => Ok(()),
            status => Err(super::super::HealthcheckError::UnexpectedStatus { status }.into()),
        }
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
