use crate::{
    dns::Resolver,
    emit,
    event::Event,
    internal_events::{ElasticSearchEventReceived, ElasticSearchMissingKeys},
    region::{region_from_endpoint, RegionOrEndpoint},
    sinks::util::{
        encoding::{EncodingConfigWithDefault, EncodingConfiguration},
        http::{BatchedHttpSink, HttpClient, HttpSink},
        retries::{RetryAction, RetryLogic},
        BatchBytesConfig, Buffer, Compression, TowerRequestConfig,
    },
    template::Template,
    tls::{TlsOptions, TlsSettings},
    topology::config::{DataType, SinkConfig, SinkContext, SinkDescription},
};
use bytes::Bytes;
use futures01::{Future, Sink};
use http::{status::StatusCode, uri::InvalidUri, Uri};
use hyper::{
    header::{HeaderName, HeaderValue},
    Body, Request,
};
use lazy_static::lazy_static;
use rusoto_core::signature::{SignedRequest, SignedRequestPayload};
use rusoto_core::{DefaultCredentialsProvider, ProvideAwsCredentials, Region};
use rusoto_credential::{AwsCredentials, CredentialsError};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use snafu::{ResultExt, Snafu};
use std::collections::HashMap;
use std::convert::TryFrom;
use tower::Service;

#[derive(Deserialize, Serialize, Debug, Clone, Default)]
#[serde(deny_unknown_fields)]
pub struct ElasticSearchConfig {
    pub host: String,
    pub index: Option<String>,
    pub doc_type: Option<String>,
    pub id_key: Option<String>,
    pub compression: Option<Compression>,
    #[serde(
        skip_serializing_if = "crate::serde::skip_serializing_if_default",
        default
    )]
    pub encoding: EncodingConfigWithDefault<Encoding>,
    #[serde(default)]
    pub batch: BatchBytesConfig,
    #[serde(default)]
    pub request: TowerRequestConfig,
    pub auth: Option<ElasticSearchAuth>,

    pub headers: Option<HashMap<String, String>>,
    pub query: Option<HashMap<String, String>>,

    pub aws: Option<RegionOrEndpoint>,
    pub tls: Option<TlsOptions>,
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
    Aws,
}

impl ElasticSearchAuth {
    pub fn apply<B>(&self, req: &mut Request<B>) {
        if let Self::Basic { user, password } = &self {
            use headers::HeaderMapExt;
            let auth = headers::Authorization::basic(&user, &password);
            req.headers_mut().typed_insert(auth);
        }
    }
}

inventory::submit! {
    SinkDescription::new::<ElasticSearchConfig>("elasticsearch")
}

#[typetag::serde(name = "elasticsearch")]
impl SinkConfig for ElasticSearchConfig {
    fn build(&self, cx: SinkContext) -> crate::Result<(super::RouterSink, super::Healthcheck)> {
        let common = ElasticSearchCommon::parse_config(&self)?;
        let healthcheck = healthcheck(cx.resolver(), &common)?;

        let batch = self.batch.unwrap_or(bytesize::mib(10u64), 1);
        let request = self.request.unwrap_with(&REQUEST_DEFAULTS);
        let tls_settings = common.tls_settings.clone();

        let gzip = common.compression == Compression::Gzip;

        let sink = BatchedHttpSink::with_retry_logic(
            common,
            Buffer::new(gzip),
            ElasticSearchRetryLogic,
            request,
            batch,
            tls_settings,
            &cx,
        )
        .sink_map_err(|e| error!("Fatal elasticsearch sink error: {}", e));

        Ok((Box::new(sink), healthcheck))
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
    authorization: Option<String>,
    credentials: Option<AwsCredentials>,
    index: Template,
    doc_type: String,
    tls_settings: TlsSettings,
    config: ElasticSearchConfig,
    compression: Compression,
    region: Region,
    query_params: HashMap<String, String>,
}

#[derive(Debug, Snafu)]
enum ParseError {
    #[snafu(display("Invalid host {:?}: {:?}", host, source))]
    InvalidHost { host: String, source: InvalidUri },
    #[snafu(display("Host {:?} must include hostname", host))]
    HostMustIncludeHostname { host: String },
    #[snafu(display("Could not create AWS credentials provider: {:?}", source))]
    AWSCredentialsProviderFailed { source: CredentialsError },
    #[snafu(display("Could not generate AWS credentials: {:?}", source))]
    AWSCredentialsGenerateFailed { source: CredentialsError },
}

impl HttpSink for ElasticSearchCommon {
    type Input = Vec<u8>;
    type Output = Vec<u8>;

    fn encode_event(&self, mut event: Event) -> Option<Self::Input> {
        self.config.encoding.apply_rules(&mut event);

        let index = self
            .index
            .render_string(&event)
            .map_err(|missing_keys| {
                emit!(ElasticSearchMissingKeys { keys: missing_keys });
            })
            .ok()?;
        info!("inserting into index: {}", index);

        let mut action = json!({
            "index": {
                "_index": index,
                "_type": self.doc_type,
            }
        });
        maybe_set_id(
            self.config.id_key.as_ref(),
            action.pointer_mut("/index").unwrap(),
            &event,
        );

        let mut body = serde_json::to_vec(&action).unwrap();
        body.push(b'\n');

        serde_json::to_writer(&mut body, &event.into_log()).unwrap();
        body.push(b'\n');

        emit!(ElasticSearchEventReceived {
            byte_size: body.len()
        });

        Some(body)
    }

    fn build_request(&self, events: Self::Output) -> http::Request<Vec<u8>> {
        let mut builder = Request::post(&self.bulk_uri);

        if let Some(credentials) = &self.credentials {
            let mut request = self.signed_request("POST", &self.bulk_uri, true);

            request.add_header("Content-Type", "application/x-ndjson");

            if let Some(headers) = &self.config.headers {
                for (header, value) in headers {
                    request.add_header(header, value);
                }
            }

            request.set_payload(Some(events));

            finish_signer(&mut request, &credentials, &mut builder);

            // The SignedRequest ends up owning the body, so we have
            // to play games here
            let body = request.payload.take().unwrap();
            match body {
                SignedRequestPayload::Buffer(body) => builder.body(body.to_vec()).unwrap(),
                _ => unreachable!(),
            }
        } else {
            builder.header("Content-Type", "application/x-ndjson");

            if self.compression == Compression::Gzip {
                builder.header("Content-Encoding", "gzip");
            }

            if let Some(headers) = &self.config.headers {
                for (header, value) in headers {
                    builder.header(&header[..], &value[..]);
                }
            }

            if let Some(auth) = &self.authorization {
                builder.header("Authorization", &auth[..]);
            }

            builder.body(events).unwrap()
        }
    }
}

#[derive(Clone)]
struct ElasticSearchRetryLogic;

impl RetryLogic for ElasticSearchRetryLogic {
    type Error = hyper::Error;
    type Response = hyper::Response<Bytes>;

    fn is_retriable_error(&self, error: &Self::Error) -> bool {
        error.is_connect() || error.is_closed()
    }

    fn should_retry_response(&self, response: &Self::Response) -> RetryAction {
        let status = response.status();

        match status {
            StatusCode::TOO_MANY_REQUESTS => RetryAction::Retry("Too many requests".into()),
            StatusCode::NOT_IMPLEMENTED => {
                RetryAction::DontRetry("endpoint not implemented".into())
            }
            _ if status.is_server_error() => RetryAction::Retry(
                format!("{}: {}", status, String::from_utf8_lossy(response.body())).into(),
            ),
            _ if status.is_success() => {
                let body = String::from_utf8_lossy(response.body());
                match body.find("\"errors\":true") {
                    Some(_) => match serde_json::from_str::<Value>(&body) {
                        Err(_) => RetryAction::DontRetry(
                            "some messages failed, and invalid response from elasticsearch".into(),
                        ),
                        Ok(_data) => RetryAction::DontRetry("some messages failed".into()),
                    },
                    None => RetryAction::Successful,
                }
            }
            _ => RetryAction::DontRetry(format!("response status: {}", status)),
        }
    }
}

impl ElasticSearchCommon {
    pub fn parse_config(config: &ElasticSearchConfig) -> crate::Result<Self> {
        let authorization = match &config.auth {
            Some(ElasticSearchAuth::Basic { user, password }) => {
                let token = format!("{}:{}", user, password);
                Some(format!("Basic {}", base64::encode(token.as_bytes())))
            }
            _ => None,
        };

        let base_url = config.host.clone();
        let region = match &config.aws {
            Some(region) => Region::try_from(region)?,
            None => region_from_endpoint(&config.host)?,
        };

        // Test the configured host, but ignore the result
        let uri = format!("{}/_test", &config.host);
        let uri = uri
            .parse::<Uri>()
            .with_context(|| InvalidHost { host: &base_url })?;
        if uri.host().is_none() {
            return Err(ParseError::HostMustIncludeHostname {
                host: config.host.clone(),
            }
            .into());
        }

        let credentials = match &config.auth {
            Some(ElasticSearchAuth::Basic { .. }) | None => None,
            Some(ElasticSearchAuth::Aws) => {
                let provider =
                    DefaultCredentialsProvider::new().context(AWSCredentialsProviderFailed)?;

                let mut rt = tokio01::runtime::current_thread::Runtime::new()?;

                let credentials = rt
                    .block_on(provider.credentials())
                    .context(AWSCredentialsGenerateFailed)?;

                Some(credentials)
            }
        };

        // Only apply compression if explicitly selected and we are
        // running with no AWS credentials.
        let compression = match (&credentials, config.compression) {
            (Some(_), _) => Compression::None,
            (_, None) => Compression::None,
            (None, Some(c)) => c,
        };

        let index = if let Some(idx) = &config.index {
            Template::from(idx.as_str())
        } else {
            Template::from("vector-%Y.%m.%d")
        };

        let doc_type = config.doc_type.clone().unwrap_or("_doc".into());

        let request = config.request.unwrap_with(&REQUEST_DEFAULTS);

        let mut query_params = config.query.clone().unwrap_or_default();
        query_params.insert("timeout".into(), format!("{}s", request.timeout.as_secs()));

        let mut query = url::form_urlencoded::Serializer::new(String::new());
        for (p, v) in &query_params {
            query.append_pair(&p[..], &v[..]);
        }
        let bulk_url = format!("{}/_bulk?{}", base_url, query.finish());
        let bulk_uri = bulk_url.parse::<Uri>().unwrap();

        let tls_settings = TlsSettings::from_options(&config.tls)?;
        let config = config.clone();

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

fn healthcheck(
    resolver: Resolver,
    common: &ElasticSearchCommon,
) -> crate::Result<super::Healthcheck> {
    let mut builder = Request::get(format!("{}/_cluster/health", common.base_url));

    match &common.credentials {
        None => {
            if let Some(authorization) = &common.authorization {
                builder.header("Authorization", authorization.clone());
            }
        }
        Some(credentials) => {
            let mut signer = common.signed_request("GET", builder.uri_ref().unwrap(), false);
            finish_signer(&mut signer, &credentials, &mut builder);
        }
    }
    let request = builder.body(Body::empty())?;

    Ok(Box::new(
        HttpClient::new(resolver, common.tls_settings.clone())?
            .call(request)
            .map_err(|err| err.into())
            .and_then(|response| match response.status() {
                hyper::StatusCode::OK => Ok(()),
                status => Err(super::HealthcheckError::UnexpectedStatus { status }.into()),
            }),
    ))
}

fn finish_signer(
    signer: &mut SignedRequest,
    credentials: &AwsCredentials,
    builder: &mut http::request::Builder,
) {
    signer.sign_with_plus(&credentials, true);

    for (name, values) in signer.headers() {
        let header_name = name
            .parse::<HeaderName>()
            .expect("Could not parse header name.");
        for value in values {
            let header_value =
                HeaderValue::from_bytes(value).expect("Could not parse header value.");
            builder.header(&header_name, header_value);
        }
    }
}

fn maybe_set_id(key: Option<impl AsRef<str>>, doc: &mut serde_json::Value, event: &Event) {
    if let Some(val) = key.and_then(|k| event.as_log().get(&k.as_ref().into())) {
        let val = val.to_string_lossy();

        doc.as_object_mut()
            .unwrap()
            .insert("_id".into(), json!(val));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sinks::util::retries::RetryAction;
    use crate::Event;
    use http::{Response, StatusCode};
    use serde_json::json;

    #[test]
    fn sets_id_from_custom_field() {
        let id_key = Some("foo");
        let mut event = Event::from("butts");
        event.as_mut_log().insert("foo", "bar");
        let mut action = json!({});

        maybe_set_id(id_key, &mut action, &event);

        assert_eq!(json!({"_id": "bar"}), action);
    }

    #[test]
    fn doesnt_set_id_when_field_missing() {
        let id_key = Some("foo");
        let mut event = Event::from("butts");
        event.as_mut_log().insert("not_foo", "bar");
        let mut action = json!({});

        maybe_set_id(id_key, &mut action, &event);

        assert_eq!(json!({}), action);
    }

    #[test]
    fn doesnt_set_id_when_not_configured() {
        let id_key: Option<&str> = None;
        let mut event = Event::from("butts");
        event.as_mut_log().insert("foo", "bar");
        let mut action = json!({});

        maybe_set_id(id_key, &mut action, &event);

        assert_eq!(json!({}), action);
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
}

#[cfg(test)]
#[cfg(feature = "es-integration-tests")]
mod integration_tests {
    use super::*;
    use crate::{
        event,
        sinks::util::http::HttpClient,
        test_util::{random_events_with_stream, random_string, runtime},
        tls::TlsOptions,
        topology::config::{SinkConfig, SinkContext},
        Event,
    };
    use futures01::{Future, Sink, Stream};
    use hyper::{Body, Request};
    use serde_json::{json, Value};
    use std::fs::File;
    use std::io::Read;
    use tower::Service;

    #[test]
    fn structures_events_correctly() {
        let mut rt = runtime();

        let index = gen_index();
        let config = ElasticSearchConfig {
            host: "http://localhost:9200".into(),
            index: Some(index.clone()),
            doc_type: Some("log_lines".into()),
            id_key: Some("my_id".into()),
            compression: Some(Compression::None),
            ..config()
        };
        let common = ElasticSearchCommon::parse_config(&config).expect("Config error");

        let cx = SinkContext::new_test(rt.executor());
        let (sink, _hc) = config.build(cx.clone()).unwrap();

        let mut input_event = Event::from("raw log line");
        input_event.as_mut_log().insert("my_id", "42");
        input_event.as_mut_log().insert("foo", "bar");

        let pump = sink.send(input_event.clone());
        rt.block_on(pump).unwrap();

        // make sure writes all all visible
        rt.block_on(flush(cx.resolver(), &common)).unwrap();

        let response = reqwest::Client::new()
            .get(&format!("{}/{}/_search", common.base_url, index))
            .json(&json!({
                "query": { "query_string": { "query": "*" } }
            }))
            .send()
            .unwrap()
            .json::<elastic_responses::search::SearchResponse<Value>>()
            .unwrap();

        assert_eq!(1, response.total());

        let hit = response.into_hits().next().unwrap();
        let doc = hit.document().unwrap();
        assert_eq!(Some("42"), doc["my_id"].as_str());

        let value = hit.into_document().unwrap();
        let expected = json!({
            "message": "raw log line",
            "my_id": "42",
            "foo": "bar",
            "timestamp": input_event.as_log()[&event::log_schema().timestamp_key()],
        });
        assert_eq!(expected, value);
    }

    #[test]
    fn insert_events_over_http() {
        run_insert_tests(
            ElasticSearchConfig {
                host: "http://localhost:9200".into(),
                doc_type: Some("log_lines".into()),
                compression: Some(Compression::None),
                ..config()
            },
            false,
        );
    }

    #[test]
    fn insert_events_over_https() {
        run_insert_tests(
            ElasticSearchConfig {
                host: "https://localhost:9201".into(),
                doc_type: Some("log_lines".into()),
                compression: Some(Compression::None),
                tls: Some(TlsOptions {
                    ca_path: Some("tests/data/Vector_CA.crt".into()),
                    ..Default::default()
                }),
                ..config()
            },
            false,
        );
    }

    #[test]
    fn insert_events_on_aws() {
        run_insert_tests(
            ElasticSearchConfig {
                auth: Some(ElasticSearchAuth::Aws),
                host: "http://localhost:4571".into(),
                ..config()
            },
            false,
        );
    }

    #[test]
    fn insert_events_with_failure() {
        run_insert_tests(
            ElasticSearchConfig {
                host: "http://localhost:9200".into(),
                doc_type: Some("log_lines".into()),
                compression: Some(Compression::None),
                ..config()
            },
            true,
        );
    }

    fn run_insert_tests(mut config: ElasticSearchConfig, break_events: bool) {
        crate::test_util::trace_init();
        let mut rt = runtime();

        let index = gen_index();
        config.index = Some(index.clone());
        let common = ElasticSearchCommon::parse_config(&config).expect("Config error");

        let cx = SinkContext::new_test(rt.executor());
        let (sink, healthcheck) = config.build(cx.clone()).expect("Building config failed");

        rt.block_on(healthcheck).expect("Health check failed");

        let (input, events) = random_events_with_stream(100, 100);
        match break_events {
            true => {
                // Break all but the first event to simulate some kind of partial failure
                let mut doit = false;
                let pump = sink.send_all(events.map(move |mut event| {
                    if doit {
                        event.as_mut_log().insert("message", 1);
                    }
                    doit = true;
                    event
                }));
                let _ = rt.block_on(pump).expect("Sending events failed");
            }
            false => {
                let pump = sink.send_all(events);
                let _ = rt.block_on(pump).expect("Sending events failed");
            }
        };

        // make sure writes all all visible
        rt.block_on(flush(cx.resolver(), &common))
            .expect("Flushing writes failed");

        let mut test_ca = Vec::<u8>::new();
        File::open("tests/data/Vector_CA.crt")
            .unwrap()
            .read_to_end(&mut test_ca)
            .unwrap();
        let test_ca = reqwest::Certificate::from_pem(&test_ca).unwrap();

        let client = reqwest::Client::builder()
            .add_root_certificate(test_ca)
            .build()
            .expect("Could not build HTTP client");

        let response = client
            .get(&format!("{}/{}/_search", common.base_url, index))
            .json(&json!({
                "query": { "query_string": { "query": "*" } }
            }))
            .send()
            .unwrap()
            .json::<elastic_responses::search::SearchResponse<Value>>()
            .unwrap();

        if break_events {
            assert_ne!(input.len() as u64, response.total());
        } else {
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
    }

    fn gen_index() -> String {
        format!("test-{}", random_string(10).to_lowercase())
    }

    fn flush(
        resolver: Resolver,
        common: &ElasticSearchCommon,
    ) -> impl Future<Item = (), Error = crate::Error> {
        let uri = format!("{}/_flush", common.base_url);
        let request = Request::post(uri).body(Body::empty()).unwrap();

        let mut client = HttpClient::new(resolver, common.tls_settings.clone())
            .expect("Could not build client to flush");
        client
            .call(request)
            .map_err(|source| source.into())
            .and_then(|response| match response.status() {
                hyper::StatusCode::OK => Ok(()),
                status => Err(super::super::HealthcheckError::UnexpectedStatus { status }.into()),
            })
    }

    fn config() -> ElasticSearchConfig {
        ElasticSearchConfig {
            batch: BatchBytesConfig {
                max_size: Some(1),
                timeout_secs: None,
            },
            ..Default::default()
        }
    }
}
