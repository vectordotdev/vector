use crate::{
    dns::Resolver,
    event::Event,
    sinks::util::{
        encoding::{skip_serializing_if_default, EncodingConfigWithDefault, EncodingConfiguration},
        http::{HttpBatchService, HttpClient, HttpRetryLogic},
        BatchBytesConfig, Buffer, Compression, SinkExt, TowerRequestConfig,
    },
    template::Template,
    tls::{TlsOptions, TlsSettings},
    topology::config::{DataType, SinkConfig, SinkContext, SinkDescription},
};
use futures01::{stream::iter_ok, Future, Sink};
use http::{uri::InvalidUri, Method, Uri};
use hyper::{
    header::{HeaderName, HeaderValue},
    Body, Request,
};
use lazy_static::lazy_static;
use rusoto_core::signature::{SignedRequest, SignedRequestPayload};
use rusoto_core::{DefaultCredentialsProvider, ProvideAwsCredentials, Region};
use rusoto_credential::{AwsCredentials, CredentialsError};
use serde::{Deserialize, Serialize};
use serde_json::json;
use snafu::{ResultExt, Snafu};
use std::collections::HashMap;
use tower::Service;

#[derive(Deserialize, Serialize, Debug, Clone, Default)]
#[serde(deny_unknown_fields)]
pub struct ElasticSearchConfig {
    pub host: String,
    pub index: Option<String>,
    pub doc_type: Option<String>,
    pub id_key: Option<String>,
    pub compression: Option<Compression>,
    #[serde(skip_serializing_if = "skip_serializing_if_default", default)]
    pub encoding: EncodingConfigWithDefault<Encoding>,
    #[serde(default)]
    pub batch: BatchBytesConfig,
    #[serde(default)]
    pub request: TowerRequestConfig,
    pub auth: Option<ElasticSearchAuth>,

    pub headers: Option<HashMap<String, String>>,
    pub query: Option<HashMap<String, String>>,

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
        let sink = es(self, common, cx);

        Ok((sink, healthcheck))
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
    authorization: Option<String>,
    credentials: Option<AwsCredentials>,
    tls_settings: TlsSettings,
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

impl ElasticSearchCommon {
    pub fn parse_config(config: &ElasticSearchConfig) -> crate::Result<Self> {
        let authorization = match &config.auth {
            Some(ElasticSearchAuth::Basic { user, password }) => {
                let token = format!("{}:{}", user, password);
                Some(format!("Basic {}", base64::encode(token.as_bytes())))
            }
            _ => None,
        };

        // let provider = config.provider.unwrap_or(Provider::Default);

        let base_url = config.host.clone();

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

                let mut rt = tokio::runtime::current_thread::Runtime::new()?;

                let credentials = rt
                    .block_on(provider.credentials())
                    .context(AWSCredentialsGenerateFailed)?;

                Some(credentials)
            }
        };

        let tls_settings = TlsSettings::from_options(&config.tls)?;

        Ok(Self {
            base_url,
            authorization,
            credentials,
            tls_settings,
        })
    }

    fn request_builder(&self, method: Method, path: &str) -> (Uri, http::request::Builder) {
        let uri = format!("{}{}", self.base_url, path);
        let uri = uri.parse::<Uri>().unwrap(); // Already tested that this parses above.
        let mut builder = Request::builder();
        builder.method(method);
        builder.uri(&uri);
        (uri, builder)
    }
}

fn es(
    config: &ElasticSearchConfig,
    common: ElasticSearchCommon,
    cx: SinkContext,
) -> super::RouterSink {
    let id_key = config.id_key.clone();
    let mut gzip = match config.compression.unwrap_or(Compression::Gzip) {
        Compression::None => false,
        Compression::Gzip => true,
    };
    let encoding = config.encoding.clone();
    let batch = config.batch.unwrap_or(bytesize::mib(10u64), 1);
    let request = config.request.unwrap_with(&REQUEST_DEFAULTS);

    let index = if let Some(idx) = &config.index {
        Template::from(idx.as_str())
    } else {
        Template::from("vector-%Y.%m.%d")
    };
    let doc_type = config.doc_type.clone().unwrap_or("_doc".into());

    let headers = config
        .headers
        .as_ref()
        .unwrap_or(&HashMap::default())
        .clone();

    let path = format!("/_bulk?timeout={}s", request.timeout.as_secs());
    let mut path_query = url::form_urlencoded::Serializer::new(path);
    if let Some(ref query) = config.query {
        for (p, v) in query {
            path_query.append_pair(&p[..], &v[..]);
        }
    }
    let path_query = path_query.finish();

    if common.credentials.is_some() {
        gzip = false;
    }

    let tls_settings = common.tls_settings.clone();
    let build_request = move |body: Vec<u8>| {
        let (uri, mut builder) = common.request_builder(Method::POST, &path_query);

        match common.credentials {
            None => {
                builder.header("Content-Type", "application/x-ndjson");
                if gzip {
                    builder.header("Content-Encoding", "gzip");
                }

                for (header, value) in &headers {
                    builder.header(&header[..], &value[..]);
                }

                if let Some(ref auth) = common.authorization {
                    builder.header("Authorization", &auth[..]);
                }

                builder.body(body).unwrap()
            }
            Some(ref credentials) => {
                let mut request = signed_request("POST", &uri);

                request.add_header("Content-Type", "application/x-ndjson");

                for (header, value) in &headers {
                    request.add_header(header, value);
                }

                request.set_payload(Some(body));

                finish_signer(&mut request, &credentials, &mut builder);

                // The SignedRequest ends up owning the body, so we have
                // to play games here
                let body = request.payload.take().unwrap();
                match body {
                    SignedRequestPayload::Buffer(body) => builder.body(body.to_vec()).unwrap(),
                    _ => unreachable!(),
                }
            }
        }
    };

    let http_service = HttpBatchService::new(cx.resolver(), tls_settings, build_request);

    let sink = request
        .batch_sink(HttpRetryLogic, http_service, cx.acker())
        .batched_with_min(Buffer::new(gzip), &batch)
        .with_flat_map(move |e| iter_ok(encode_event(e, &index, &doc_type, &id_key, &encoding)));

    Box::new(sink)
}

fn encode_event(
    mut event: Event,
    index: &Template,
    doc_type: &str,
    id_key: &Option<String>,
    encoding: &EncodingConfigWithDefault<Encoding>,
) -> Option<Vec<u8>> {
    encoding.apply_rules(&mut event);
    let index = index
        .render_string(&event)
        .map_err(|missing_keys| {
            warn!(
                message = "Keys do not exist on the event; Dropping event.",
                ?missing_keys,
                rate_limit_secs = 30,
            );
        })
        .ok()?;

    let mut action = json!({
        "index": {
            "_index": index,
            "_type": doc_type,
        }
    });
    maybe_set_id(
        id_key.as_ref(),
        action.pointer_mut("/index").unwrap(),
        &event,
    );

    let mut body = serde_json::to_vec(&action).unwrap();
    body.push(b'\n');

    serde_json::to_writer(&mut body, &event.into_log()).unwrap();
    body.push(b'\n');
    Some(body)
}

fn healthcheck(
    resolver: Resolver,
    common: &ElasticSearchCommon,
) -> crate::Result<super::Healthcheck> {
    let (uri, mut builder) = common.request_builder(Method::GET, "/_cluster/health");
    match &common.credentials {
        None => {
            if let Some(authorization) = &common.authorization {
                builder.header("Authorization", authorization.clone());
            }
        }
        Some(credentials) => {
            let mut signer = signed_request("GET", &uri);
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

fn signed_request(method: &str, uri: &Uri) -> SignedRequest {
    let region = Region::Custom {
        name: "custom".into(),
        endpoint: uri.to_string(),
    };
    SignedRequest::new(method, "es", &region, uri.path())
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
    use crate::Event;
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
    use futures01::{Future, Sink};
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
        run_insert_tests(ElasticSearchConfig {
            host: "http://localhost:9200".into(),
            doc_type: Some("log_lines".into()),
            compression: Some(Compression::None),
            ..config()
        });
    }

    #[test]
    fn insert_events_over_https() {
        run_insert_tests(ElasticSearchConfig {
            host: "https://localhost:9201".into(),
            doc_type: Some("log_lines".into()),
            compression: Some(Compression::None),
            tls: Some(TlsOptions {
                ca_path: Some("tests/data/Vector_CA.crt".into()),
                ..Default::default()
            }),
            ..config()
        });
    }

    #[test]
    fn insert_events_on_aws() {
        run_insert_tests(ElasticSearchConfig {
            auth: Some(ElasticSearchAuth::Aws),
            host: "http://localhost:4571".into(),
            ..config()
        });
    }

    fn run_insert_tests(mut config: ElasticSearchConfig) {
        let mut rt = runtime();

        let index = gen_index();
        config.index = Some(index.clone());
        let common = ElasticSearchCommon::parse_config(&config).expect("Config error");

        let cx = SinkContext::new_test(rt.executor());
        let (sink, healthcheck) = config.build(cx.clone()).expect("Building config failed");

        rt.block_on(healthcheck).expect("Health check failed");

        let (input, events) = random_events_with_stream(100, 100);

        let pump = sink.send_all(events);
        let _ = rt.block_on(pump).expect("Sending events failed");

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
