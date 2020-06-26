use crate::{
    dns::Resolver,
    event::{self, Event},
    sinks::util::{
        encoding::{EncodingConfig, EncodingConfiguration},
        http::{Auth, BatchedHttpSink, HttpClient, HttpSink},
        service2::TowerRequestConfig,
        BatchBytesConfig, Buffer, Compression, UriSerde,
    },
    tls::{TlsOptions, TlsSettings},
    topology::config::{DataType, SinkConfig, SinkContext, SinkDescription},
};
use futures::{FutureExt, TryFutureExt};
use futures01::{future, Sink};
use http::{
    header::{self, HeaderName, HeaderValue},
    Method, Request, StatusCode, Uri,
};
use hyper::Body;
use indexmap::IndexMap;
use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
use snafu::{ResultExt, Snafu};

#[derive(Debug, Snafu)]
enum BuildError {
    #[snafu(display("{}: {}", source, name))]
    InvalidHeaderName {
        name: String,
        source: header::InvalidHeaderName,
    },
    #[snafu(display("{}: {}", source, value))]
    InvalidHeaderValue {
        value: String,
        source: header::InvalidHeaderValue,
    },
}

#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct HttpSinkConfig {
    pub uri: UriSerde,
    pub method: Option<HttpMethod>,
    pub healthcheck_uri: Option<UriSerde>,
    pub auth: Option<Auth>,
    pub headers: Option<IndexMap<String, String>>,
    #[serde(default)]
    pub compression: Compression,
    pub encoding: EncodingConfig<Encoding>,
    #[serde(default)]
    pub batch: BatchBytesConfig,
    #[serde(default)]
    pub request: TowerRequestConfig,
    pub tls: Option<TlsOptions>,
}

#[cfg(test)]
fn default_config(e: Encoding) -> HttpSinkConfig {
    HttpSinkConfig {
        uri: Default::default(),
        method: Default::default(),
        healthcheck_uri: Default::default(),
        auth: Default::default(),
        headers: Default::default(),
        compression: Default::default(),
        batch: Default::default(),
        encoding: e.into(),
        request: Default::default(),
        tls: Default::default(),
    }
}

lazy_static! {
    static ref REQUEST_DEFAULTS: TowerRequestConfig = TowerRequestConfig {
        in_flight_limit: Some(10),
        timeout_secs: Some(30),
        rate_limit_num: Some(10),
        ..Default::default()
    };
}

#[derive(Deserialize, Serialize, Debug, Eq, PartialEq, Clone, Derivative)]
#[serde(rename_all = "snake_case")]
#[derivative(Default)]
pub enum HttpMethod {
    #[derivative(Default)]
    Post,
    Put,
}

#[derive(Deserialize, Serialize, Debug, Eq, PartialEq, Clone)]
#[serde(rename_all = "snake_case")]
pub enum Encoding {
    Text,
    Ndjson,
    Json,
}

inventory::submit! {
    SinkDescription::new_without_default::<HttpSinkConfig>("http")
}

#[typetag::serde(name = "http")]
impl SinkConfig for HttpSinkConfig {
    fn build(&self, cx: SinkContext) -> crate::Result<(super::RouterSink, super::Healthcheck)> {
        validate_headers(&self.headers, &self.auth)?;
        let tls = TlsSettings::from_options(&self.tls)?;

        let mut config = self.clone();
        config.uri = build_uri(config.uri.clone()).into();

        let compression = config.compression;
        let batch = config.batch.unwrap_or(bytesize::mib(10u64), 1);
        let request = config.request.unwrap_with(&REQUEST_DEFAULTS);

        let sink = BatchedHttpSink::new(
            config,
            Buffer::new(compression),
            request,
            batch,
            Some(tls.clone()),
            &cx,
        )
        .sink_map_err(|e| error!("Fatal http sink error: {}", e));

        let sink = Box::new(sink);

        match self.healthcheck_uri.clone() {
            Some(healthcheck_uri) => {
                let healthcheck =
                    healthcheck(healthcheck_uri, self.auth.clone(), cx.resolver(), tls)
                        .boxed()
                        .compat();
                Ok((sink, Box::new(healthcheck)))
            }
            None => Ok((sink, Box::new(future::ok(())))),
        }
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }

    fn sink_type(&self) -> &'static str {
        "http"
    }
}

#[async_trait::async_trait]
impl HttpSink for HttpSinkConfig {
    type Input = Vec<u8>;
    type Output = Vec<u8>;

    fn encode_event(&self, mut event: Event) -> Option<Self::Input> {
        self.encoding.apply_rules(&mut event);
        let event = event.into_log();

        let body = match &self.encoding.codec() {
            Encoding::Text => {
                if let Some(v) = event.get(&event::log_schema().message_key()) {
                    let mut b = v.to_string_lossy().into_bytes();
                    b.push(b'\n');
                    b
                } else {
                    warn!(
                        message = "Event missing the message key; Dropping event.",
                        rate_limit_secs = 30,
                    );
                    return None;
                }
            }

            Encoding::Ndjson => {
                let mut b = serde_json::to_vec(&event)
                    .map_err(|e| panic!("Unable to encode into JSON: {}", e))
                    .ok()?;
                b.push(b'\n');
                b
            }

            Encoding::Json => {
                let mut b = serde_json::to_vec(&event)
                    .map_err(|e| panic!("Unable to encode into JSON: {}", e))
                    .ok()?;
                b.push(b',');
                b
            }
        };

        Some(body)
    }

    async fn build_request(&self, mut body: Self::Output) -> crate::Result<http::Request<Vec<u8>>> {
        let method = match &self.method.clone().unwrap_or(HttpMethod::Post) {
            HttpMethod::Post => Method::POST,
            HttpMethod::Put => Method::PUT,
        };
        let uri: Uri = self.uri.clone().into();

        let ct = match self.encoding.codec() {
            Encoding::Text => "text/plain",
            Encoding::Ndjson => "application/x-ndjson",
            Encoding::Json => {
                body.insert(0, b'[');
                body.pop(); // remove trailing comma from last record
                body.push(b']');
                "application/json"
            }
        };

        let mut builder = Request::builder()
            .method(method)
            .uri(uri)
            .header("Content-Type", ct);

        if let Some(ce) = self.compression.content_encoding() {
            builder = builder.header("Content-Encoding", ce);
        }

        if let Some(headers) = &self.headers {
            for (header, value) in headers.iter() {
                builder = builder.header(header.as_str(), value.as_str());
            }
        }

        let mut request = builder.body(body).unwrap();

        if let Some(auth) = &self.auth {
            auth.apply(&mut request);
        }

        Ok(request)
    }
}

async fn healthcheck(
    uri: UriSerde,
    auth: Option<Auth>,
    resolver: Resolver,
    tls_settings: TlsSettings,
) -> crate::Result<()> {
    let uri = build_uri(uri);
    let mut request = Request::head(&uri).body(Body::empty()).unwrap();

    if let Some(auth) = auth {
        auth.apply(&mut request);
    }

    let mut client = HttpClient::new(resolver, tls_settings)?;
    let response = client.send(request).await?;

    match response.status() {
        StatusCode::OK => Ok(()),
        status => Err(super::HealthcheckError::UnexpectedStatus { status }.into()),
    }
}

fn validate_headers(
    headers: &Option<IndexMap<String, String>>,
    auth: &Option<Auth>,
) -> crate::Result<()> {
    if let Some(map) = headers {
        for (name, value) in map {
            if auth.is_some() && name.eq_ignore_ascii_case("Authorization") {
                return Err(
                    "Authorization header can not be used with defined auth options".into(),
                );
            }

            HeaderName::from_bytes(name.as_bytes()).with_context(|| InvalidHeaderName { name })?;
            HeaderValue::from_bytes(value.as_bytes())
                .with_context(|| InvalidHeaderValue { value })?;
        }
    }
    Ok(())
}

fn build_uri(base: UriSerde) -> Uri {
    let base: Uri = base.into();
    Uri::builder()
        .scheme(base.scheme_str().unwrap_or("http"))
        .authority(base.authority().map(|a| a.as_str()).unwrap_or("127.0.0.1"))
        .path_and_query(base.path_and_query().map(|pq| pq.as_str()).unwrap_or(""))
        .build()
        .expect("bug building uri")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        assert_downcast_matches,
        sinks::http::HttpSinkConfig,
        sinks::util::http::HttpSink,
        sinks::util::test::build_test_server,
        test_util::{next_addr, random_lines_with_stream, runtime, shutdown_on_idle},
        topology::config::SinkContext,
    };
    use futures01::{Sink, Stream};
    use headers03::{Authorization, HeaderMapExt};
    use hyper::Method;
    use serde::Deserialize;
    use std::io::{BufRead, BufReader};

    #[test]
    fn http_encode_event_text() {
        let encoding = EncodingConfig::from(Encoding::Text);
        let event = Event::from("hello world");

        let mut config = default_config(Encoding::Text);
        config.encoding = encoding.clone();
        let bytes = config.encode_event(event).unwrap();

        assert_eq!(bytes, Vec::from(&"hello world\n"[..]));
    }

    #[test]
    fn http_encode_event_json() {
        let encoding = EncodingConfig::from(Encoding::Ndjson);
        let event = Event::from("hello world");

        let mut config = default_config(Encoding::Json);
        config.encoding = encoding.clone();
        let bytes = config.encode_event(event).unwrap();

        #[derive(Deserialize, Debug)]
        #[serde(deny_unknown_fields)]
        struct ExpectedEvent {
            message: String,
            timestamp: chrono::DateTime<chrono::Utc>,
        }

        let output = serde_json::from_slice::<ExpectedEvent>(&bytes[..]).unwrap();

        assert_eq!(output.message, "hello world".to_string());
    }

    #[test]
    fn http_validates_normal_headers() {
        let config = r#"
        uri = "http://$IN_ADDR/frames"
        encoding = "text"
        [headers]
        Auth = "token:thing_and-stuff"
        X-Custom-Nonsense = "_%_{}_-_&_._`_|_~_!_#_&_$_"
        "#;
        let config: HttpSinkConfig = toml::from_str(&config).unwrap();

        assert!(super::validate_headers(&config.headers, &None).is_ok());
    }

    #[test]
    fn http_catches_bad_header_names() {
        let config = r#"
        uri = "http://$IN_ADDR/frames"
        encoding = "text"
        [headers]
        "\u0001" = "bad"
        "#;
        let config: HttpSinkConfig = toml::from_str(&config).unwrap();

        assert_downcast_matches!(
            super::validate_headers(&config.headers, &None).unwrap_err(),
            BuildError,
            BuildError::InvalidHeaderName{..}
        );
    }

    #[test]
    #[should_panic(expected = "Authorization header can not be used with defined auth options")]
    fn http_headers_auth_conflict() {
        let config = r#"
        uri = "http://$IN_ADDR/"
        encoding = "text"
        [headers]
        Authorization = "Basic base64encodedstring"
        [auth]
        strategy = "basic"
        user = "user"
        password = "password"
        "#;
        let config: HttpSinkConfig = toml::from_str(&config).unwrap();

        let rt = runtime();
        let cx = SinkContext::new_test(rt.executor());

        let _ = config.build(cx).unwrap();
    }

    #[test]
    fn http_happy_path_post() {
        let num_lines = 1000;

        let in_addr = next_addr();

        let config = r#"
        uri = "http://$IN_ADDR/frames"
        compression = "gzip"
        encoding = "ndjson"

        [auth]
        strategy = "basic"
        user = "waldo"
        password = "hunter2"
    "#
        .replace("$IN_ADDR", &format!("{}", in_addr));
        let config: HttpSinkConfig = toml::from_str(&config).unwrap();

        let mut rt = runtime();
        let cx = SinkContext::new_test(rt.executor());

        let (sink, _) = config.build(cx).unwrap();
        let (rx, trigger, server) = build_test_server(in_addr, &mut rt);

        let (input_lines, events) = random_lines_with_stream(100, num_lines);
        let pump = sink.send_all(events);

        rt.spawn(server);

        let _ = rt.block_on(pump).unwrap();
        drop(trigger);

        let output_lines = rx
            .wait()
            .map(Result::unwrap)
            .map(|(parts, body)| {
                assert_eq!(Method::POST, parts.method);
                assert_eq!("/frames", parts.uri.path());
                assert_eq!(
                    Some(Authorization::basic("waldo", "hunter2")),
                    parts.headers.typed_get()
                );
                body
            })
            .map(std::io::Cursor::new)
            .map(flate2::read::GzDecoder::new)
            .map(BufReader::new)
            .flat_map(BufRead::lines)
            .map(Result::unwrap)
            .map(|s| {
                let val: serde_json::Value = serde_json::from_str(&s).unwrap();
                val.get("message").unwrap().as_str().unwrap().to_owned()
            })
            .collect::<Vec<_>>();

        shutdown_on_idle(rt);

        assert_eq!(num_lines, output_lines.len());
        assert_eq!(input_lines, output_lines);
    }

    #[test]
    fn http_happy_path_put() {
        let num_lines = 1000;

        let in_addr = next_addr();

        let config = r#"
        uri = "http://$IN_ADDR/frames"
        method = "put"
        compression = "gzip"
        encoding = "ndjson"

        [auth]
        strategy = "basic"
        user = "waldo"
        password = "hunter2"
    "#
        .replace("$IN_ADDR", &format!("{}", in_addr));
        let config: HttpSinkConfig = toml::from_str(&config).unwrap();

        let mut rt = runtime();
        let cx = SinkContext::new_test(rt.executor());

        let (sink, _) = config.build(cx).unwrap();
        let (rx, trigger, server) = build_test_server(in_addr, &mut rt);

        let (input_lines, events) = random_lines_with_stream(100, num_lines);
        let pump = sink.send_all(events);

        rt.spawn(server);

        let _ = rt.block_on(pump).unwrap();
        drop(trigger);

        let output_lines = rx
            .wait()
            .map(Result::unwrap)
            .map(|(parts, body)| {
                assert_eq!(Method::PUT, parts.method);
                assert_eq!("/frames", parts.uri.path());
                assert_eq!(
                    Some(Authorization::basic("waldo", "hunter2")),
                    parts.headers.typed_get()
                );
                body
            })
            .map(std::io::Cursor::new)
            .map(flate2::read::GzDecoder::new)
            .map(BufReader::new)
            .flat_map(BufRead::lines)
            .map(Result::unwrap)
            .map(|s| {
                let val: serde_json::Value = serde_json::from_str(&s).unwrap();
                val.get("message").unwrap().as_str().unwrap().to_owned()
            })
            .collect::<Vec<_>>();

        shutdown_on_idle(rt);

        assert_eq!(num_lines, output_lines.len());
        assert_eq!(input_lines, output_lines);
    }

    #[test]
    fn http_passes_custom_headers() {
        let num_lines = 1000;

        let in_addr = next_addr();

        let config = r#"
        uri = "http://$IN_ADDR/frames"
        encoding = "ndjson"
        compression = "gzip"
        [headers]
        foo = "bar"
        baz = "quux"
    "#
        .replace("$IN_ADDR", &format!("{}", in_addr));
        let config: HttpSinkConfig = toml::from_str(&config).unwrap();

        let mut rt = runtime();
        let cx = SinkContext::new_test(rt.executor());

        let (sink, _) = config.build(cx).unwrap();
        let (rx, trigger, server) = build_test_server(in_addr, &mut rt);

        let (input_lines, events) = random_lines_with_stream(100, num_lines);
        let pump = sink.send_all(events);

        rt.spawn(server);

        let _ = rt.block_on(pump).unwrap();
        drop(trigger);

        let output_lines = rx
            .wait()
            .map(Result::unwrap)
            .map(|(parts, body)| {
                assert_eq!(Method::POST, parts.method);
                assert_eq!("/frames", parts.uri.path());
                assert_eq!(
                    Some("bar"),
                    parts.headers.get("foo").map(|v| v.to_str().unwrap())
                );
                assert_eq!(
                    Some("quux"),
                    parts.headers.get("baz").map(|v| v.to_str().unwrap())
                );
                body
            })
            .map(std::io::Cursor::new)
            .map(flate2::read::GzDecoder::new)
            .map(BufReader::new)
            .flat_map(BufRead::lines)
            .map(Result::unwrap)
            .map(|s| {
                let val: serde_json::Value = serde_json::from_str(&s).unwrap();
                val.get("message").unwrap().as_str().unwrap().to_owned()
            })
            .collect::<Vec<_>>();

        shutdown_on_idle(rt);

        assert_eq!(num_lines, output_lines.len());
        assert_eq!(input_lines, output_lines);
    }
}
