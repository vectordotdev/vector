use crate::{
    buffers::Acker,
    event::{self, Event},
    sinks::util::{
        http::{HttpRetryLogic, HttpService},
        retries::FixedRetryPolicy,
        BatchServiceSink, Buffer, Compression, SinkExt,
    },
};
use futures::{future, Future, Sink};
use headers::HeaderMapExt;
use http::{
    header::{HeaderName, HeaderValue},
    Method, Uri,
};
use hyper::{Body, Client, Request};
use hyper_tls::HttpsConnector;
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tower::ServiceBuilder;

#[derive(Deserialize, Serialize, Clone, Debug, Default)]
#[serde(deny_unknown_fields)]
pub struct HttpSinkConfig {
    pub uri: String,
    pub method: Option<HttpMethod>,
    pub healthcheck_uri: Option<String>,
    #[serde(flatten)]
    pub basic_auth: Option<BasicAuth>,
    pub headers: Option<IndexMap<String, String>>,
    pub batch_size: Option<usize>,
    pub batch_timeout: Option<u64>,
    pub compression: Option<Compression>,
    pub encoding: Encoding,

    // Tower Request based configuration
    pub request_in_flight_limit: Option<usize>,
    pub request_timeout_secs: Option<u64>,
    pub request_rate_limit_duration_secs: Option<u64>,
    pub request_rate_limit_num: Option<u64>,
    pub request_retry_attempts: Option<usize>,
    pub request_retry_backoff_secs: Option<u64>,
}

#[derive(Deserialize, Serialize, Debug, Eq, PartialEq, Clone, Derivative)]
#[serde(rename_all = "snake_case")]
#[derivative(Default)]
pub enum HttpMethod {
    #[derivative(Default)]
    Post,
    Put,
}

#[derive(Deserialize, Serialize, Debug, Eq, PartialEq, Clone, Derivative)]
#[serde(rename_all = "snake_case")]
#[derivative(Default)]
pub enum Encoding {
    #[derivative(Default)]
    Text,
    Ndjson,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct BasicAuth {
    user: String,
    password: String,
}

#[typetag::serde(name = "http")]
impl crate::topology::config::SinkConfig for HttpSinkConfig {
    fn build(&self, acker: Acker) -> Result<(super::RouterSink, super::Healthcheck), String> {
        validate_headers(&self.headers)?;
        let sink = http(self.clone(), acker)?;

        if let Some(healthcheck_uri) = self.healthcheck_uri.clone() {
            let healtcheck = healthcheck(healthcheck_uri, self.basic_auth.clone())?;
            Ok((sink, healtcheck))
        } else {
            Ok((sink, Box::new(future::ok(()))))
        }
    }
}

fn http(config: HttpSinkConfig, acker: Acker) -> Result<super::RouterSink, String> {
    let uri = build_uri(&config.uri)?;

    let gzip = match config.compression.unwrap_or(Compression::None) {
        Compression::None => false,
        Compression::Gzip => true,
    };
    let batch_timeout = config.batch_timeout.unwrap_or(1);
    let batch_size = config.batch_size.unwrap_or(bytesize::mib(10u64) as usize);

    let timeout = config.request_timeout_secs.unwrap_or(30);
    let in_flight_limit = config.request_in_flight_limit.unwrap_or(10);
    let rate_limit_duration = config.request_rate_limit_duration_secs.unwrap_or(1);
    let rate_limit_num = config.request_rate_limit_num.unwrap_or(10);
    let retry_attempts = config.request_retry_attempts.unwrap_or(usize::max_value());
    let retry_backoff_secs = config.request_retry_backoff_secs.unwrap_or(1);
    let encoding = config.encoding.clone();
    let headers = config.headers.clone();
    let basic_auth = config.basic_auth.clone();
    let method = config.method.clone().unwrap_or(HttpMethod::Post);

    let policy = FixedRetryPolicy::new(
        retry_attempts,
        Duration::from_secs(retry_backoff_secs),
        HttpRetryLogic,
    );

    let http_service = HttpService::new(move |body: Vec<u8>| {
        let mut builder = hyper::Request::builder();

        let method = match method {
            HttpMethod::Post => Method::POST,
            HttpMethod::Put => Method::PUT,
        };

        builder.method(method);

        builder.uri(uri.clone());

        match encoding {
            Encoding::Text => builder.header("Content-Type", "text/plain"),
            Encoding::Ndjson => builder.header("Content-Type", "application/x-ndjson"),
        };

        if gzip {
            builder.header("Content-Encoding", "gzip");
        }

        if let Some(headers) = &headers {
            for (header, value) in headers.iter() {
                builder.header(header.as_str(), value.as_str());
            }
        }

        let mut request = builder.body(body).unwrap();

        if let Some(auth) = &basic_auth {
            auth.apply(request.headers_mut());
        }

        request
    });

    let service = ServiceBuilder::new()
        .concurrency_limit(in_flight_limit)
        .rate_limit(rate_limit_num, Duration::from_secs(rate_limit_duration))
        .retry(policy)
        .timeout(Duration::from_secs(timeout))
        .service(http_service);

    let encoding = config.encoding.clone();
    let sink = BatchServiceSink::new(service, acker)
        .batched_with_min(
            Buffer::new(gzip),
            batch_size,
            Duration::from_secs(batch_timeout),
        )
        .with(move |event| encode_event(event, &encoding));

    Ok(Box::new(sink))
}

fn healthcheck(uri: String, auth: Option<BasicAuth>) -> Result<super::Healthcheck, String> {
    let uri = build_uri(&uri)?;
    let mut request = Request::head(&uri).body(Body::empty()).unwrap();

    if let Some(auth) = auth {
        auth.apply(request.headers_mut());
    }

    let https = HttpsConnector::new(4).expect("TLS initialization failed");
    let client = Client::builder().build(https);

    let healthcheck = client
        .request(request)
        .map_err(|err| err.to_string())
        .and_then(|response| {
            use hyper::StatusCode;

            match response.status() {
                StatusCode::OK => Ok(()),
                other => Err(format!("Unexpected status: {}", other)),
            }
        });

    Ok(Box::new(healthcheck))
}

impl BasicAuth {
    fn apply(&self, header_map: &mut http::header::HeaderMap) {
        let auth = headers::Authorization::basic(&self.user, &self.password);
        header_map.typed_insert(auth)
    }
}

fn validate_headers(headers: &Option<IndexMap<String, String>>) -> Result<(), String> {
    if let Some(map) = headers {
        for (name, value) in map {
            HeaderName::from_bytes(name.as_bytes()).map_err(|e| format!("{}: {}", e, name))?;
            HeaderValue::from_bytes(value.as_bytes()).map_err(|e| format!("{}: {}", e, value))?;
        }
    }
    Ok(())
}

fn build_uri(raw: &str) -> Result<Uri, String> {
    let base: Uri = raw
        .parse()
        .map_err(|e| format!("invalid uri ({}): {:?}", e, raw))?;
    Ok(Uri::builder()
        .scheme(base.scheme_str().unwrap_or("http"))
        .authority(
            base.authority_part()
                .map(|a| a.as_str())
                .unwrap_or("127.0.0.1"),
        )
        .path_and_query(base.path_and_query().map(|pq| pq.as_str()).unwrap_or(""))
        .build()
        .expect("bug building uri"))
}

fn encode_event(event: Event, encoding: &Encoding) -> Result<Vec<u8>, ()> {
    let event = event.into_log();

    let mut body = match encoding {
        Encoding::Text => event
            .get(&event::MESSAGE)
            .map(|v| v.to_string_lossy().into_bytes())
            .unwrap_or(Vec::new()),

        Encoding::Ndjson => serde_json::to_vec(&event.all_fields())
            .map_err(|e| panic!("Unable to encode into JSON: {}", e))?,
    };

    body.push(b'\n');

    Ok(body)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buffers::Acker;
    use crate::{
        sinks::http::HttpSinkConfig,
        test_util::{next_addr, random_lines_with_stream, shutdown_on_idle},
        topology::config::SinkConfig,
    };
    use bytes::Buf;
    use futures::{sync::mpsc, Future, Sink, Stream};
    use headers::{Authorization, HeaderMapExt};
    use hyper::service::service_fn_ok;
    use hyper::{Body, Request, Response, Server};
    use serde::Deserialize;
    use std::io::{BufRead, BufReader};

    #[test]
    fn http_encode_event_text() {
        let encoding = Encoding::Text;
        let event = Event::from("hello world");

        let bytes = encode_event(event, &encoding).unwrap();

        assert_eq!(bytes, Vec::from(&"hello world\n"[..]));
    }

    #[test]
    fn http_encode_event_json() {
        let encoding = Encoding::Ndjson;
        let event = Event::from("hello world");

        let bytes = encode_event(event, &encoding).unwrap();

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

        assert_eq!(Ok(()), super::validate_headers(&config.headers));
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

        assert_eq!(
            Err(String::from("invalid HTTP header name: \u{1}")),
            super::validate_headers(&config.headers)
        );
    }

    #[test]
    fn http_happy_path() {
        let num_lines = 1000;

        let in_addr = next_addr();

        let config = r#"
        uri = "http://$IN_ADDR/frames"
        user = "waldo"
        compression = "gzip"
        password = "hunter2"
        encoding = "ndjson"
    "#
        .replace("$IN_ADDR", &format!("{}", in_addr));
        let config: HttpSinkConfig = toml::from_str(&config).unwrap();

        let (sink, _healthcheck) = config.build(Acker::Null).unwrap();
        let (rx, trigger, server) = build_test_server(&in_addr);

        let (input_lines, events) = random_lines_with_stream(100, num_lines);
        let pump = sink.send_all(events);

        let mut rt = tokio::runtime::Runtime::new().unwrap();
        rt.spawn(server);

        rt.block_on(pump).unwrap();
        drop(trigger);

        let output_lines = rx
            .wait()
            .map(Result::unwrap)
            .map(|(parts, body)| {
                assert_eq!(hyper::Method::POST, parts.method);
                assert_eq!("/frames", parts.uri.path());
                assert_eq!(
                    Some(Authorization::basic("waldo", "hunter2")),
                    parts.headers.typed_get()
                );
                body
            })
            .map(hyper::Chunk::reader)
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

        let (sink, _healthcheck) = config.build(Acker::Null).unwrap();
        let (rx, trigger, server) = build_test_server(&in_addr);

        let (input_lines, events) = random_lines_with_stream(100, num_lines);
        let pump = sink.send_all(events);

        let mut rt = tokio::runtime::Runtime::new().unwrap();
        rt.spawn(server);

        rt.block_on(pump).unwrap();
        drop(trigger);

        let output_lines = rx
            .wait()
            .map(Result::unwrap)
            .map(|(parts, body)| {
                assert_eq!(hyper::Method::POST, parts.method);
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
            .map(hyper::Chunk::reader)
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

    fn build_test_server(
        addr: &std::net::SocketAddr,
    ) -> (
        mpsc::Receiver<(http::request::Parts, hyper::Chunk)>,
        stream_cancel::Trigger,
        impl Future<Item = (), Error = ()>,
    ) {
        let (tx, rx) = mpsc::channel(100);
        let service = move || {
            let tx = tx.clone();
            service_fn_ok(move |req: Request<Body>| {
                let (parts, body) = req.into_parts();

                let tx = tx.clone();
                tokio::spawn(
                    body.concat2()
                        .map_err(|e| panic!(e))
                        .and_then(|body| tx.send((parts, body)))
                        .map(|_| ())
                        .map_err(|e| panic!(e)),
                );

                Response::new(Body::empty())
            })
        };

        let (trigger, tripwire) = stream_cancel::Tripwire::new();
        let server = Server::bind(addr)
            .serve(service)
            .with_graceful_shutdown(tripwire)
            .map_err(|e| panic!("server error: {}", e));

        (rx, trigger, server)
    }
}
