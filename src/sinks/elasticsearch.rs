use crate::{
    buffers::Acker,
    event::Event,
    region::RegionOrEndpoint,
    sinks::util::{
        http::{https_client, HttpRetryLogic, HttpService},
        retries::FixedRetryPolicy,
        tls::{TlsOptions, TlsSettings},
        BatchServiceSink, Buffer, Compression, SinkExt,
    },
    template::Template,
    topology::config::{DataType, SinkConfig},
};
use futures::{stream::iter_ok, Future, Sink};
use http::{uri::InvalidUri, Method, Uri};
use hyper::{
    header::{HeaderName, HeaderValue},
    Body, Request,
};
use rusoto_core::signature::{SignedRequest, SignedRequestPayload};
use rusoto_core::{DefaultCredentialsProvider, ProvideAwsCredentials, Region};
use rusoto_credential::{AwsCredentials, CredentialsError};
use serde::{Deserialize, Serialize};
use serde_json::json;
use snafu::{ResultExt, Snafu};
use std::collections::HashMap;
use std::convert::TryInto;
use std::time::Duration;
use tower::ServiceBuilder;

#[derive(Deserialize, Serialize, Debug, Clone, Default)]
#[serde(deny_unknown_fields)]
pub struct ElasticSearchConfig {
    pub host: String,
    pub index: Option<String>,
    pub doc_type: Option<String>,
    pub id_key: Option<String>,
    pub batch_size: Option<usize>,
    pub batch_timeout: Option<u64>,
    pub compression: Option<Compression>,
    pub provider: Option<Provider>,
    pub region: Option<RegionOrEndpoint>,

    // Tower Request based configuration
    pub request_in_flight_limit: Option<usize>,
    pub request_timeout_secs: Option<u64>,
    pub request_rate_limit_duration_secs: Option<u64>,
    pub request_rate_limit_num: Option<u64>,
    pub request_retry_attempts: Option<usize>,
    pub request_retry_backoff_secs: Option<u64>,

    pub basic_auth: Option<ElasticSearchBasicAuthConfig>,

    pub headers: Option<HashMap<String, String>>,
    pub query: Option<HashMap<String, String>>,

    pub tls: Option<TlsOptions>,
}

#[derive(Deserialize, Serialize, Debug, Clone, Default)]
#[serde(deny_unknown_fields)]
pub struct ElasticSearchBasicAuthConfig {
    pub password: String,
    pub user: String,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(rename_all = "snake_case")]
pub enum Provider {
    Default,
    Aws,
}

#[typetag::serde(name = "elasticsearch")]
impl SinkConfig for ElasticSearchConfig {
    fn build(&self, acker: Acker) -> crate::Result<(super::RouterSink, super::Healthcheck)> {
        let common = ElasticSearchCommon::parse_config(&self)?;
        let healthcheck = healthcheck(&common)?;
        let sink = es(self, common, acker);

        Ok((sink, healthcheck))
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }
}

struct ElasticSearchCommon {
    host: String,
    authorization: Option<String>,
    region: Option<Region>,
    credentials: Option<AwsCredentials>,
    tls_settings: TlsSettings,
}

#[derive(Debug, Snafu)]
enum ParseError {
    #[snafu(display("Invalid host {:?}: {:?}", host, source))]
    InvalidHost { host: String, source: InvalidUri },
    #[snafu(display("AWS provider requires a configured region"))]
    AWSRequiresRegion,
    #[snafu(display("Could not create AWS credentials provider: {:?}", source))]
    AWSCredentialsProviderFailed { source: CredentialsError },
    #[snafu(display("Could not generate AWS credentials: {:?}", source))]
    AWSCredentialsGenerateFailed { source: CredentialsError },
}

impl ElasticSearchCommon {
    fn parse_config(config: &ElasticSearchConfig) -> crate::Result<Self> {
        // Test the configured host, but ignore the result
        let uri = format!("{}/_test", config.host);
        uri.parse::<Uri>().with_context(|| InvalidHost {
            host: config.host.clone(),
        })?;

        let authorization = config.basic_auth.as_ref().map(|auth| {
            let token = format!("{}:{}", auth.user, auth.password);
            format!("Basic {}", base64::encode(token.as_bytes()))
        });

        let region: Option<Region> = match config.region {
            Some(ref region) => Some(region.try_into()?),
            None => None,
        };

        let credentials = match config.provider.as_ref().unwrap_or(&Provider::Default) {
            Provider::Default => None,
            Provider::Aws => {
                if region.is_none() {
                    return Err(ParseError::AWSRequiresRegion.into());
                }
                Some(
                    DefaultCredentialsProvider::new()
                        .context(AWSCredentialsProviderFailed)?
                        .credentials()
                        .wait()
                        .context(AWSCredentialsGenerateFailed)?,
                )
            }
        };

        let tls_settings = TlsSettings::from_options(&config.tls)?;

        Ok(Self {
            host: config.host.clone(),
            authorization,
            region,
            credentials,
            tls_settings,
        })
    }

    fn request_builder(&self, method: Method, path: &str) -> (Uri, http::request::Builder) {
        let uri = format!("{}{}", self.host, path);
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
    acker: Acker,
) -> super::RouterSink {
    let id_key = config.id_key.clone();
    let mut gzip = match config.compression.unwrap_or(Compression::Gzip) {
        Compression::None => false,
        Compression::Gzip => true,
    };

    let batch_size = config.batch_size.unwrap_or(bytesize::mib(10u64) as usize);
    let batch_timeout = config.batch_timeout.unwrap_or(1);

    let timeout = config.request_timeout_secs.unwrap_or(60);
    let in_flight_limit = config.request_in_flight_limit.unwrap_or(5);
    let rate_limit_duration = config.request_rate_limit_duration_secs.unwrap_or(1);
    let rate_limit_num = config.request_rate_limit_num.unwrap_or(5);
    let retry_attempts = config.request_retry_attempts.unwrap_or(usize::max_value());
    let retry_backoff_secs = config.request_retry_backoff_secs.unwrap_or(1);

    let index = if let Some(idx) = &config.index {
        Template::from(idx.as_str())
    } else {
        Template::from("vector-%Y.%m.%d")
    };
    let doc_type = config.doc_type.clone().unwrap_or("_doc".into());

    let policy = FixedRetryPolicy::new(
        retry_attempts,
        Duration::from_secs(retry_backoff_secs),
        HttpRetryLogic,
    );

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

    let http_service = HttpService::builder()
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
                        SignedRequestPayload::Buffer(body) => builder.body(body).unwrap(),
                        _ => unreachable!(),
                    }
                }
            }
        });

    let service = ServiceBuilder::new()
        .concurrency_limit(in_flight_limit)
        .rate_limit(rate_limit_num, Duration::from_secs(rate_limit_duration))
        .retry(policy)
        .timeout(Duration::from_secs(timeout))
        .service(http_service);

    let sink = BatchServiceSink::new(service, acker)
        .batched_with_min(
            Buffer::new(gzip),
            batch_size,
            Duration::from_secs(batch_timeout),
        )
        .with_flat_map(move |e| iter_ok(encode_event(e, &index, &doc_type, &id_key)));

    Box::new(sink)
}

fn encode_event(
    event: Event,
    index: &Template,
    doc_type: &String,
    id_key: &Option<String>,
) -> Option<Vec<u8>> {
    let index = index
        .render_string(&event)
        .map_err(|keys| {
            warn!(
                message = "Keys do not exist on the event. Dropping event.",
                ?keys
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
    use crate::Event;
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
}

#[cfg(test)]
#[cfg(feature = "es-integration-tests")]
mod integration_tests {
    use super::*;
    use crate::buffers::Acker;
    use crate::{
        event,
        sinks::util::http::https_client,
        sinks::util::tls::TlsOptions,
        test_util::{block_on, random_events_with_stream, random_string},
        topology::config::SinkConfig,
        Event,
    };
    use elastic::client::SyncClientBuilder;
    use futures::{Future, Sink};
    use hyper::{Body, Request};
    use serde_json::{json, Value};
    use std::fs::File;
    use std::io::Read;

    #[test]
    fn structures_events_correctly() {
        let index = gen_index();
        let config = ElasticSearchConfig {
            host: "http://localhost:9200/".into(),
            index: Some(index.clone()),
            doc_type: Some("log_lines".into()),
            id_key: Some("my_id".into()),
            compression: Some(Compression::None),
            batch_size: Some(1),
            ..Default::default()
        };

        let (sink, _hc) = config.build(Acker::Null).unwrap();

        let mut input_event = Event::from("raw log line");
        input_event
            .as_mut_log()
            .insert_explicit("my_id".into(), "42".into());
        input_event
            .as_mut_log()
            .insert_explicit("foo".into(), "bar".into());

        let pump = sink.send(input_event.clone());
        block_on(pump).unwrap();

        // make sure writes all all visible
        block_on(flush(&config)).unwrap();

        let client = SyncClientBuilder::new().build().unwrap();

        let response = client
            .search::<Value>()
            .index(index)
            .body(json!({
                "query": { "query_string": { "query": "*" } }
            }))
            .send()
            .unwrap();
        assert_eq!(1, response.total());

        let hit = response.into_hits().next().unwrap();
        assert_eq!("42", hit.id());

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
            host: "http://localhost:9200".into(),
            doc_type: Some("log_lines".into()),
            compression: Some(Compression::None),
            batch_size: Some(1),
            ..Default::default()
        });
    }

    #[test]
    fn insert_events_over_https() {
        run_insert_tests(ElasticSearchConfig {
            host: "https://localhost:9201".into(),
            doc_type: Some("log_lines".into()),
            compression: Some(Compression::None),
            batch_size: Some(1),
            tls: Some(TlsOptions {
                ca_path: Some("tests/data/Vector_CA.crt".into()),
                ..Default::default()
            }),
            ..Default::default()
        });
    }

    #[test]
    fn insert_events_on_aws() {
        let url = "http://localhost:4571";
        run_insert_tests(ElasticSearchConfig {
            host: url.into(),
            batch_size: Some(1),
            provider: Some(Provider::Aws),
            region: Some(RegionOrEndpoint::with_endpoint(url.into())),
            ..Default::default()
        });
    }

    fn run_insert_tests(mut config: ElasticSearchConfig) {
        let index = gen_index();
        config.index = Some(index.clone());

        let (sink, healthcheck) = config.build(Acker::Null).expect("Building config failed");

        block_on(healthcheck).expect("Health check failed");

        let (input, events) = random_events_with_stream(100, 100);

        let pump = sink.send_all(events);
        block_on(pump).expect("Sending events failed");

        // make sure writes all all visible
        block_on(flush(&config)).expect("Flushing writes failed");

        let mut test_ca = Vec::<u8>::new();
        File::open("tests/data/Vector_CA.crt")
            .unwrap()
            .read_to_end(&mut test_ca)
            .unwrap();
        let test_ca = reqwest::Certificate::from_pem(&test_ca).unwrap();

        let http_client = reqwest::Client::builder()
            .add_root_certificate(test_ca)
            .build()
            .expect("Could not build HTTP client");
        let client = SyncClientBuilder::new()
            .http_client(http_client)
            .static_node(config.host)
            .build()
            .expect("Building test client failed");

        let response = client
            .search::<Value>()
            .index(index)
            .body(json!({
                "query": { "query_string": { "query": "*" } }
            }))
            .send()
            .expect("Issuing test query failed");

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

    fn flush(config: &ElasticSearchConfig) -> impl Future<Item = (), Error = crate::Error> {
        let uri = format!("{}/_flush", config.host);
        let request = Request::post(uri).body(Body::empty()).unwrap();

        let common = ElasticSearchCommon::parse_config(config).expect("Config error");
        https_client(common.tls_settings)
            .expect("Could not build client to flush")
            .request(request)
            .map_err(|source| dbg!(source).into())
            .and_then(|response| match response.status() {
                hyper::StatusCode::OK => Ok(()),
                status => Err(super::super::HealthcheckError::UnexpectedStatus { status }.into()),
            })
    }
}
