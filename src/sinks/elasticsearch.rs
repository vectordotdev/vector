use crate::{
    event::Event,
    region::{self, RegionOrEndpoint},
    sinks::util::{
        http::{https_client, HttpRetryLogic, HttpService},
        tls::{TlsOptions, TlsSettings},
        BatchConfig, Buffer, Compression, SinkExt, TowerRequestConfig,
    },
    template::Template,
    topology::config::{DataType, SinkConfig, SinkContext, SinkDescription},
};
use futures::{stream::iter_ok, Future, Sink};
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
use std::convert::TryInto;

#[derive(Deserialize, Serialize, Debug, Clone, Default)]
#[serde(deny_unknown_fields)]
pub struct ElasticSearchConfig {
    pub host: Option<String>,
    pub index: Option<String>,
    pub doc_type: Option<String>,
    pub id_key: Option<String>,
    pub compression: Option<Compression>,
    pub provider: Option<Provider>,
    #[serde(default, flatten)]
    pub batch: BatchConfig,
    // TODO: This should be an Option, but when combined with flatten we never seem to get back
    // a None. For now, we get optionality by handling the error during parsing when nothing is
    // passed. See https://github.com/timberio/vector/issues/1160
    #[serde(flatten)]
    pub region: RegionOrEndpoint,
    #[serde(flatten)]
    pub request: TowerRequestConfig,
    pub basic_auth: Option<ElasticSearchBasicAuthConfig>,

    pub headers: Option<HashMap<String, String>>,
    pub query: Option<HashMap<String, String>>,

    pub tls: Option<TlsOptions>,
}

lazy_static! {
    static ref REQUEST_DEFAULTS: TowerRequestConfig = TowerRequestConfig {
        ..Default::default()
    };
}

#[derive(Deserialize, Serialize, Debug, Clone, Default)]
#[serde(deny_unknown_fields)]
pub struct ElasticSearchBasicAuthConfig {
    pub password: String,
    pub user: String,
}

#[derive(Deserialize, Serialize, Debug, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub enum Provider {
    Default,
    Aws,
}

inventory::submit! {
    SinkDescription::new::<ElasticSearchConfig>("elasticsearch")
}

#[typetag::serde(name = "elasticsearch")]
impl SinkConfig for ElasticSearchConfig {
    fn build(&self, cx: SinkContext) -> crate::Result<(super::RouterSink, super::Healthcheck)> {
        let common = ElasticSearchCommon::parse_config(&self)?;
        let healthcheck = healthcheck(&common)?;
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

struct ElasticSearchCommon {
    base_url: String,
    authorization: Option<String>,
    region: Option<Region>,
    credentials: Option<AwsCredentials>,
    tls_settings: TlsSettings,
}

#[derive(Debug, Snafu)]
enum ParseError {
    #[snafu(display("Invalid host {:?}: {:?}", host, source))]
    InvalidHost { host: String, source: InvalidUri },
    #[snafu(display("Default provider requires a configured host"))]
    DefaultRequiresHost,
    #[snafu(display("AWS provider requires a configured region"))]
    AWSRequiresRegion,
    #[snafu(display("Could not create AWS credentials provider: {:?}", source))]
    AWSCredentialsProviderFailed { source: CredentialsError },
    #[snafu(display("Could not generate AWS credentials: {:?}", source))]
    AWSCredentialsGenerateFailed { source: CredentialsError },
}

impl ElasticSearchCommon {
    fn parse_config(config: &ElasticSearchConfig) -> crate::Result<Self> {
        let authorization = config.basic_auth.as_ref().map(|auth| {
            let token = format!("{}:{}", auth.user, auth.password);
            format!("Basic {}", base64::encode(token.as_bytes()))
        });

        let region: Option<Region> = match (&config.region).try_into() {
            Ok(region) => Some(region),
            Err(region::ParseError::MissingRegionAndEndpoint) => None,
            Err(error) => return Err(error.into()),
        };

        let provider = config.provider.unwrap_or(Provider::Default);

        let base_url = match provider {
            Provider::Default => match config.host {
                Some(ref host) => host.clone(),
                None => return Err(ParseError::DefaultRequiresHost.into()),
            },
            Provider::Aws => match region {
                None => return Err(ParseError::AWSRequiresRegion.into()),
                Some(ref region) => match region {
                    // Adapted from rusoto_core::signature::build_hostname, which is unfortunately not pub
                    Region::Custom { endpoint, .. } if endpoint.contains("://") => endpoint.clone(),
                    Region::Custom { endpoint, .. } => format!("https://{}", endpoint),
                    Region::CnNorth1 | Region::CnNorthwest1 => {
                        format!("https://es.{}.amazonaws.com.cn", region.name())
                    }
                    _ => format!("https://es.{}.amazonaws.com", region.name()),
                },
            },
        };

        // Test the configured host, but ignore the result
        let uri = format!("{}/_test", base_url);
        uri.parse::<Uri>()
            .with_context(|| InvalidHost { host: &base_url })?;

        let credentials = match provider {
            Provider::Default => None,
            Provider::Aws => {
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
            region,
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

    let mut path_query = url::form_urlencoded::Serializer::new(String::from("/_bulk"));
    if let Some(ref query) = config.query {
        for (p, v) in query {
            path_query.append_pair(&p[..], &v[..]);
        }
    }
    let path_query = path_query.finish();

    if common.credentials.is_some() {
        gzip = false;
    }

    let http_service = HttpService::builder(cx.resolver())
        .tls_settings(common.tls_settings.clone())
        .build(move |body: Vec<u8>| {
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
                    let mut request = SignedRequest::new(
                        "POST",
                        "es",
                        common.region.as_ref().unwrap(),
                        uri.path(),
                    );
                    request.set_hostname(uri.host().map(|s| s.into()));

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
        });

    let sink = request
        .batch_sink(HttpRetryLogic, http_service, cx.acker())
        .batched_with_min(Buffer::new(gzip), &batch)
        .with_flat_map(move |e| iter_ok(encode_event(e, &index, &doc_type, &id_key)));

    Box::new(sink)
}

fn encode_event(
    event: Event,
    index: &Template,
    doc_type: &str,
    id_key: &Option<String>,
) -> Option<Vec<u8>> {
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

    serde_json::to_writer(&mut body, &event.into_log().unflatten()).unwrap();
    body.push(b'\n');
    Some(body)
}

fn healthcheck(common: &ElasticSearchCommon) -> crate::Result<super::Healthcheck> {
    let (uri, mut builder) = common.request_builder(Method::GET, "/_cluster/health");
    match &common.credentials {
        None => {
            if let Some(authorization) = &common.authorization {
                builder.header("Authorization", authorization.clone());
            }
        }
        Some(credentials) => {
            let mut signer =
                SignedRequest::new("GET", "es", common.region.as_ref().unwrap(), uri.path());
            signer.set_hostname(uri.host().map(|s| s.into()));
            finish_signer(&mut signer, &credentials, &mut builder);
        }
    }
    let request = builder.body(Body::empty())?;

    Ok(Box::new(
        https_client(common.tls_settings.clone())?
            .request(request)
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
    use crate::{assert_downcast_matches, Event};
    use serde_json::json;

    #[test]
    fn sets_id_from_custom_field() {
        let id_key = Some("foo");
        let mut event = Event::from("butts");
        event
            .as_mut_log()
            .insert_explicit("foo".into(), "bar".into());
        let mut action = json!({});

        maybe_set_id(id_key, &mut action, &event);

        assert_eq!(json!({"_id": "bar"}), action);
    }

    #[test]
    fn doesnt_set_id_when_field_missing() {
        let id_key = Some("foo");
        let mut event = Event::from("butts");
        event
            .as_mut_log()
            .insert_explicit("not_foo".into(), "bar".into());
        let mut action = json!({});

        maybe_set_id(id_key, &mut action, &event);

        assert_eq!(json!({}), action);
    }

    #[test]
    fn doesnt_set_id_when_not_configured() {
        let id_key: Option<&str> = None;
        let mut event = Event::from("butts");
        event
            .as_mut_log()
            .insert_explicit("foo".into(), "bar".into());
        let mut action = json!({});

        maybe_set_id(id_key, &mut action, &event);

        assert_eq!(json!({}), action);
    }

    fn parse_config(input: &str) -> crate::Result<ElasticSearchCommon> {
        let config: ElasticSearchConfig = toml::from_str(input).unwrap();
        ElasticSearchCommon::parse_config(&config)
    }

    fn parse_config_err(input: &str) -> crate::Error {
        // ElasticSearchCommon doesn't impl Debug, so can't just unwrap_err
        match parse_config(input) {
            Ok(_) => panic!("Mis-parsed invalid config"),
            Err(err) => err,
        }
    }

    #[test]
    fn host_is_required_for_default() {
        let err = parse_config_err(r#"provider = "default""#);
        assert_downcast_matches!(err, ParseError, ParseError::DefaultRequiresHost);
    }

    #[test]
    fn host_is_not_required_for_aws() {
        let result = parse_config(
            r#"
                provider = "aws"
                region = "us-east-1"
            "#,
        );
        // If not running in an AWS context, this will fail with a
        // credentials error, but that is valid too.
        match result {
            Ok(_) => (),
            Err(err) => {
                assert_downcast_matches!(err, ParseError, ParseError::AWSCredentialsGenerateFailed { .. })
            }
        }
    }

    #[test]
    fn region_is_not_required_for_default() {
        let common = parse_config(r#"host = "https://example.com""#).unwrap();

        assert_eq!(None, common.region);
    }

    #[test]
    fn region_is_required_for_aws() {
        let err = parse_config_err(r#"provider = "aws""#);
        assert_downcast_matches!(err, ParseError, ParseError::AWSRequiresRegion { .. });
    }
}

#[cfg(test)]
#[cfg(feature = "es-integration-tests")]
mod integration_tests {
    use super::*;
    use crate::{
        event,
        sinks::util::http::https_client,
        sinks::util::tls::TlsOptions,
        test_util::{random_events_with_stream, random_string, runtime},
        topology::config::{SinkConfig, SinkContext},
        Event,
    };
    use futures::{Future, Sink};
    use hyper::{Body, Request};
    use serde_json::{json, Value};
    use std::fs::File;
    use std::io::Read;

    #[test]
    fn structures_events_correctly() {
        let mut rt = runtime();

        let index = gen_index();
        let config = ElasticSearchConfig {
            host: Some("http://localhost:9200".into()),
            index: Some(index.clone()),
            doc_type: Some("log_lines".into()),
            id_key: Some("my_id".into()),
            compression: Some(Compression::None),
            ..config()
        };
        let common = ElasticSearchCommon::parse_config(&config).expect("Config error");

        let (sink, _hc) = config.build(SinkContext::new_test(rt.executor())).unwrap();

        let mut input_event = Event::from("raw log line");
        input_event
            .as_mut_log()
            .insert_explicit("my_id".into(), "42".into());
        input_event
            .as_mut_log()
            .insert_explicit("foo".into(), "bar".into());

        let pump = sink.send(input_event.clone());
        rt.block_on(pump).unwrap();

        // make sure writes all all visible
        rt.block_on(flush(&common)).unwrap();

        let response = reqwest::Client::new()
            .get(&format!("{}/{}/_search", common.base_url, index))
            .json(&json!({
                "query": { "query_string": { "query": "*" } }
            }))
            .send()
            .unwrap()
            .json::<elastic_responses::search::SearchResponse<Value>>()
            .unwrap();

        println!("response {:?}", response);

        assert_eq!(1, response.total());

        let hit = response.into_hits().next().unwrap();
        let doc = hit.document().unwrap();
        assert_eq!(Some("42"), doc["my_id"].as_str());

        let value = hit.into_document().unwrap();
        let expected = json!({
            "message": "raw log line",
            "my_id": "42",
            "foo": "bar",
            "timestamp": input_event.as_log()[&event::TIMESTAMP],
        });
        assert_eq!(expected, value);
    }

    #[test]
    fn insert_events_over_http() {
        run_insert_tests(ElasticSearchConfig {
            host: Some("http://localhost:9200".into()),
            doc_type: Some("log_lines".into()),
            compression: Some(Compression::None),
            ..config()
        });
    }

    #[test]
    fn insert_events_over_https() {
        run_insert_tests(ElasticSearchConfig {
            host: Some("https://localhost:9201".into()),
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
            provider: Some(Provider::Aws),
            region: RegionOrEndpoint::with_endpoint("http://localhost:4571".into()),
            ..config()
        });
    }

    fn run_insert_tests(mut config: ElasticSearchConfig) {
        let mut rt = runtime();

        let index = gen_index();
        config.index = Some(index.clone());
        let common = ElasticSearchCommon::parse_config(&config).expect("Config error");

        let (sink, healthcheck) = config
            .build(SinkContext::new_test(rt.executor()))
            .expect("Building config failed");

        rt.block_on(healthcheck).expect("Health check failed");

        let (input, events) = random_events_with_stream(100, 100);

        let pump = sink.send_all(events);
        let _ = rt.block_on(pump).expect("Sending events failed");

        // make sure writes all all visible
        rt.block_on(flush(&common)).expect("Flushing writes failed");

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
            .map(|rec| serde_json::to_value(rec.into_log().unflatten()).unwrap())
            .collect::<Vec<_>>();
        for hit in response.into_hits() {
            let event = hit.into_document().unwrap();
            assert!(input.contains(&event));
        }
    }

    fn gen_index() -> String {
        format!("test-{}", random_string(10).to_lowercase())
    }

    fn flush(common: &ElasticSearchCommon) -> impl Future<Item = (), Error = crate::Error> {
        let uri = format!("{}/_flush", common.base_url);
        let request = Request::post(uri).body(Body::empty()).unwrap();

        https_client(common.tls_settings.clone())
            .expect("Could not build client to flush")
            .request(request)
            .map_err(|source| dbg!(source).into())
            .and_then(|response| match response.status() {
                hyper::StatusCode::OK => Ok(()),
                status => Err(super::super::HealthcheckError::UnexpectedStatus { status }.into()),
            })
    }

    fn config() -> ElasticSearchConfig {
        ElasticSearchConfig {
            batch: BatchConfig {
                batch_size: Some(1),
                batch_timeout: None,
            },
            ..Default::default()
        }
    }
}
