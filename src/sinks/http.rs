use std::io::Write;

use bytes::{BufMut, Bytes, BytesMut};
use flate2::write::GzEncoder;
use futures::{future, FutureExt, SinkExt};
use http::{
    header::{self, HeaderName, HeaderValue},
    Method, Request, StatusCode, Uri,
};
use hyper::Body;
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use snafu::{ResultExt, Snafu};
use tokio_util::codec::Encoder;

use crate::{
    codecs::{
        encoding::{self, FramingConfig, SerializerConfig},
        CharacterDelimitedEncoder, CharacterDelimitedEncoderConfig, JsonSerializerConfig,
        NewlineDelimitedEncoder, NewlineDelimitedEncoderConfig, RawMessageSerializerConfig,
    },
    config::{
        AcknowledgementsConfig, GenerateConfig, Input, SinkConfig, SinkContext, SinkDescription,
    },
    event::Event,
    http::{Auth, HttpClient, MaybeAuth},
    internal_events::HttpEventEncoded,
    sinks::util::{
        self,
        encoding::{EncodingConfig, EncodingConfigAdapter, EncodingConfigMigrator, Transformer},
        http::{BatchedHttpSink, HttpEventEncoder, RequestConfig},
        BatchConfig, Buffer, Compression, RealtimeSizeBasedDefaultBatchSettings,
        TowerRequestConfig, UriSerde,
    },
    tls::{TlsOptions, TlsSettings},
};

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Migrator;

impl EncodingConfigMigrator for Migrator {
    type Codec = Encoding;

    fn migrate(codec: &Self::Codec) -> (Option<FramingConfig>, SerializerConfig) {
        match codec {
            Encoding::Text => (None, RawMessageSerializerConfig::new().into()),
            Encoding::Ndjson => (
                Some(NewlineDelimitedEncoderConfig::new().into()),
                JsonSerializerConfig::new().into(),
            ),
            Encoding::Json => (
                Some(CharacterDelimitedEncoderConfig::new(b',').into()),
                JsonSerializerConfig::new().into(),
            ),
        }
    }
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct HttpSinkConfig {
    pub uri: UriSerde,
    pub method: Option<HttpMethod>,
    pub auth: Option<Auth>,
    // Deprecated, moved to request.
    pub headers: Option<IndexMap<String, String>>,
    #[serde(default)]
    pub compression: Compression,
    #[serde(flatten)]
    pub encoding: EncodingConfigAdapter<EncodingConfig<Encoding>, Migrator>,
    #[serde(default)]
    pub batch: BatchConfig<RealtimeSizeBasedDefaultBatchSettings>,
    #[serde(default)]
    pub request: RequestConfig,
    pub tls: Option<TlsOptions>,
    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::skip_serializing_if_default"
    )]
    pub acknowledgements: AcknowledgementsConfig,
}

#[derive(Deserialize, Serialize, Debug, Eq, PartialEq, Clone, Derivative)]
#[serde(rename_all = "snake_case")]
#[derivative(Default)]
pub enum HttpMethod {
    #[derivative(Default)]
    Get,
    Head,
    Post,
    Put,
    Delete,
    Options,
    Trace,
    Patch,
}

#[derive(Deserialize, Serialize, Debug, Eq, PartialEq, Clone)]
#[serde(rename_all = "snake_case")]
pub enum Encoding {
    Text,
    Ndjson,
    Json,
}

inventory::submit! {
    SinkDescription::new::<HttpSinkConfig>("http")
}

impl GenerateConfig for HttpSinkConfig {
    fn generate_config() -> toml::Value {
        toml::from_str(
            r#"uri = "https://10.22.212.22:9000/endpoint"
            encoding.codec = "json""#,
        )
        .unwrap()
    }
}

impl HttpSinkConfig {
    fn build_http_client(&self, cx: &SinkContext) -> crate::Result<HttpClient> {
        let tls = TlsSettings::from_options(&self.tls)?;
        Ok(HttpClient::new(tls, cx.proxy())?)
    }
}

struct HttpSink {
    pub uri: UriSerde,
    pub method: Option<HttpMethod>,
    pub auth: Option<Auth>,
    pub compression: Compression,
    pub transformer: Transformer,
    pub encoder: encoding::Encoder,
    pub batch: BatchConfig<RealtimeSizeBasedDefaultBatchSettings>,
    pub request: RequestConfig,
}

#[cfg(test)]
fn default_sink(encoding: Encoding) -> HttpSink {
    let encoding =
        EncodingConfigAdapter::<EncodingConfig<Encoding>, Migrator>::legacy(encoding.into())
            .encoding();
    let framing = encoding
        .0
        .unwrap_or_else(|| NewlineDelimitedEncoder::new().into());
    let serializer = encoding.1;
    let encoder = encoding::Encoder::new(framing, serializer);

    HttpSink {
        uri: Default::default(),
        method: Default::default(),
        auth: Default::default(),
        compression: Default::default(),
        transformer: Default::default(),
        encoder,
        batch: Default::default(),
        request: Default::default(),
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "http")]
impl SinkConfig for HttpSinkConfig {
    async fn build(
        &self,
        cx: SinkContext,
    ) -> crate::Result<(super::VectorSink, super::Healthcheck)> {
        let client = self.build_http_client(&cx)?;

        let healthcheck = match cx.healthcheck.uri.clone() {
            Some(healthcheck_uri) => {
                healthcheck(healthcheck_uri, self.auth.clone(), client.clone()).boxed()
            }
            None => future::ok(()).boxed(),
        };

        let mut request = self.request.clone();
        request.add_old_option(self.headers.clone());
        validate_headers(&request.headers, &self.auth)?;

        let encoding = self.encoding.clone().encoding();
        let framing = encoding
            .0
            .unwrap_or_else(|| NewlineDelimitedEncoder::new().into());
        let serializer = encoding.1;
        let encoder = encoding::Encoder::new(framing, serializer);

        let sink = HttpSink {
            uri: self.uri.with_default_parts(),
            method: self.method.clone(),
            auth: self.auth.choose_one(&self.uri.auth)?,
            compression: self.compression,
            transformer: self.encoding.transformer(),
            encoder,
            batch: self.batch,
            request,
        };

        let request = sink
            .request
            .tower
            .unwrap_with(&TowerRequestConfig::default());

        let batch = sink.batch.into_batch_settings()?;
        let sink = BatchedHttpSink::new(
            sink,
            Buffer::new(batch.size, Compression::None),
            request,
            batch.timeout,
            client,
            cx.acker(),
        )
        .sink_map_err(|error| error!(message = "Fatal HTTP sink error.", %error));

        let sink = super::VectorSink::from_event_sink(sink);

        Ok((sink, healthcheck))
    }

    fn input(&self) -> Input {
        Input::log()
    }

    fn sink_type(&self) -> &'static str {
        "http"
    }

    fn acknowledgements(&self) -> Option<&AcknowledgementsConfig> {
        Some(&self.acknowledgements)
    }
}

pub struct HttpSinkEventEncoder {
    encoder: encoding::Encoder,
    transformer: Transformer,
}

impl HttpEventEncoder<BytesMut> for HttpSinkEventEncoder {
    fn encode_event(&mut self, mut event: Event) -> Option<BytesMut> {
        self.transformer.transform(&mut event);

        let mut body = BytesMut::new();
        self.encoder.encode(event, &mut body).ok()?;

        emit!(&HttpEventEncoded {
            byte_size: body.len(),
        });

        Some(body)
    }
}

#[async_trait::async_trait]
impl util::http::HttpSink for HttpSink {
    type Input = BytesMut;
    type Output = BytesMut;
    type Encoder = HttpSinkEventEncoder;

    fn build_encoder(&self) -> Self::Encoder {
        HttpSinkEventEncoder {
            encoder: self.encoder.clone(),
            transformer: self.transformer.clone(),
        }
    }

    async fn build_request(&self, mut body: Self::Output) -> crate::Result<http::Request<Bytes>> {
        let method = match &self.method.clone().unwrap_or(HttpMethod::Post) {
            HttpMethod::Get => Method::GET,
            HttpMethod::Head => Method::HEAD,
            HttpMethod::Post => Method::POST,
            HttpMethod::Put => Method::PUT,
            HttpMethod::Delete => Method::DELETE,
            HttpMethod::Options => Method::OPTIONS,
            HttpMethod::Trace => Method::TRACE,
            HttpMethod::Patch => Method::PATCH,
        };
        let uri: Uri = self.uri.uri.clone();

        let content_type = {
            use encoding::{Framer::*, Serializer::*};
            match (self.encoder.serializer(), self.encoder.framer()) {
                (RawMessage(_), _) => Some("text/plain"),
                (Json(_), NewlineDelimited(_)) => {
                    if !body.is_empty() {
                        // Remove trailing newline for backwards-compatibility
                        // with Vector `0.20.x`.
                        body.truncate(body.len() - 1);
                    }
                    Some("application/x-ndjson")
                }
                (Json(_), CharacterDelimited(CharacterDelimitedEncoder { delimiter: b',' })) => {
                    // TODO(https://github.com/vectordotdev/vector/issues/11253):
                    // Prepend before building a request body to eliminate the
                    // additional copy here.
                    let message = body.split();
                    body.put_u8(b'[');
                    if !message.is_empty() {
                        body.unsplit(message);
                        // remove trailing comma from last record
                        body.truncate(body.len() - 1);
                    }
                    body.put_u8(b']');

                    Some("application/json")
                }
                _ => None,
            }
        };

        let mut builder = Request::builder().method(method).uri(uri);

        if let Some(content_type) = content_type {
            builder = builder.header("Content-Type", content_type);
        }

        match self.compression {
            Compression::Gzip(level) => {
                builder = builder.header("Content-Encoding", "gzip");

                let buffer = BytesMut::new();
                let mut w = GzEncoder::new(buffer.writer(), level);
                w.write_all(&body).expect("Writing to Vec can't fail");
                body = w.finish().expect("Writing to Vec can't fail").into_inner();
            }
            Compression::None => {}
        }

        for (header, value) in self.request.headers.iter() {
            builder = builder.header(header.as_str(), value.as_str());
        }

        let mut request = builder.body(body.freeze()).unwrap();

        if let Some(auth) = &self.auth {
            auth.apply(&mut request);
        }

        Ok(request)
    }
}

async fn healthcheck(uri: UriSerde, auth: Option<Auth>, client: HttpClient) -> crate::Result<()> {
    let auth = auth.choose_one(&uri.auth)?;
    let uri = uri.with_default_parts();
    let mut request = Request::head(&uri.uri).body(Body::empty()).unwrap();

    if let Some(auth) = auth {
        auth.apply(&mut request);
    }

    let response = client.send(request).await?;

    match response.status() {
        StatusCode::OK => Ok(()),
        status => Err(super::HealthcheckError::UnexpectedStatus { status }.into()),
    }
}

fn validate_headers(map: &IndexMap<String, String>, auth: &Option<Auth>) -> crate::Result<()> {
    for (name, value) in map {
        if auth.is_some() && name.eq_ignore_ascii_case("Authorization") {
            return Err("Authorization header can not be used with defined auth options".into());
        }

        HeaderName::from_bytes(name.as_bytes())
            .with_context(|_| InvalidHeaderNameSnafu { name })?;
        HeaderValue::from_bytes(value.as_bytes())
            .with_context(|_| InvalidHeaderValueSnafu { value })?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{
        io::{BufRead, BufReader},
        sync::{atomic, Arc},
    };

    use bytes::{Buf, Bytes};
    use flate2::read::MultiGzDecoder;
    use futures::{channel::mpsc, stream, StreamExt};
    use headers::{Authorization, HeaderMapExt};
    use http::request::Parts;
    use hyper::{Method, Response, StatusCode};
    use serde::Deserialize;
    use vector_core::event::{BatchNotifier, BatchStatus};

    use super::*;
    use crate::{
        assert_downcast_matches,
        config::SinkContext,
        sinks::util::{
            http::HttpSink,
            test::{build_test_server, build_test_server_generic, build_test_server_status},
        },
        test_util::{components, components::HTTP_SINK_TAGS, next_addr, random_lines_with_stream},
    };

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<HttpSinkConfig>();
    }

    #[test]
    fn http_encode_event_text() {
        let event = Event::from("hello world");

        let sink = default_sink(Encoding::Text);
        let mut encoder = sink.build_encoder();
        let bytes = encoder.encode_event(event).unwrap();

        assert_eq!(bytes, Vec::from("hello world\n"));
    }

    #[test]
    fn http_encode_event_ndjson() {
        let event = Event::from("hello world");

        let sink = default_sink(Encoding::Ndjson);
        let mut encoder = sink.build_encoder();
        let bytes = encoder.encode_event(event).unwrap();

        #[derive(Deserialize, Debug)]
        #[serde(deny_unknown_fields)]
        #[allow(dead_code)] // deserialize all fields
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
        [request.headers]
        Auth = "token:thing_and-stuff"
        X-Custom-Nonsense = "_%_{}_-_&_._`_|_~_!_#_&_$_"
        "#;
        let config: HttpSinkConfig = toml::from_str(config).unwrap();

        assert!(super::validate_headers(&config.request.headers, &None).is_ok());
    }

    #[test]
    fn http_catches_bad_header_names() {
        let config = r#"
        uri = "http://$IN_ADDR/frames"
        encoding = "text"
        [request.headers]
        "\u0001" = "bad"
        "#;
        let config: HttpSinkConfig = toml::from_str(config).unwrap();

        assert_downcast_matches!(
            super::validate_headers(&config.request.headers, &None).unwrap_err(),
            BuildError,
            BuildError::InvalidHeaderName { .. }
        );
    }

    // TODO: Fix failure on GH Actions using macos-latest image.
    #[cfg(not(target_os = "macos"))]
    #[tokio::test]
    #[should_panic(expected = "Authorization header can not be used with defined auth options")]
    async fn http_headers_auth_conflict() {
        let config = r#"
        uri = "http://$IN_ADDR/"
        encoding = "text"
        [request.headers]
        Authorization = "Basic base64encodedstring"
        [auth]
        strategy = "basic"
        user = "user"
        password = "password"
        "#;
        let config: HttpSinkConfig = toml::from_str(config).unwrap();

        let cx = SinkContext::new_test();

        let _ = config.build(cx).await.unwrap();
    }

    #[tokio::test]
    async fn http_happy_path_post() {
        run_sink(
            r#"
        [auth]
        strategy = "basic"
        user = "waldo"
        password = "hunter2"
    "#,
            |parts| {
                assert_eq!(Method::POST, parts.method);
                assert_eq!("/frames", parts.uri.path());
                assert_eq!(
                    Some(Authorization::basic("waldo", "hunter2")),
                    parts.headers.typed_get()
                );
            },
        )
        .await;
    }

    #[tokio::test]
    async fn http_happy_path_put() {
        run_sink(
            r#"
        method = "put"

        [auth]
        strategy = "basic"
        user = "waldo"
        password = "hunter2"
    "#,
            |parts| {
                assert_eq!(Method::PUT, parts.method);
                assert_eq!("/frames", parts.uri.path());
                assert_eq!(
                    Some(Authorization::basic("waldo", "hunter2")),
                    parts.headers.typed_get()
                );
            },
        )
        .await;
    }

    #[tokio::test]
    async fn http_passes_custom_headers() {
        run_sink(
            r#"
        [request.headers]
        foo = "bar"
        baz = "quux"
    "#,
            |parts| {
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
            },
        )
        .await;
    }

    #[tokio::test]
    async fn retries_on_no_connection() {
        let num_lines = 10;

        let (in_addr, sink) = build_sink("").await;

        let (batch, mut receiver) = BatchNotifier::new_with_receiver();
        let (input_lines, events) = random_lines_with_stream(100, num_lines, Some(batch));
        let pump = tokio::spawn(sink.run(events));

        // This ordering starts the sender before the server has built
        // its accepting socket. The delay below ensures that the sink
        // attempts to connect at least once before creating the
        // listening socket.
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        let (rx, trigger, server) = build_test_server(in_addr);
        tokio::spawn(server);

        pump.await.unwrap().unwrap();
        drop(trigger);

        assert_eq!(receiver.try_recv(), Ok(BatchStatus::Delivered));

        let output_lines = get_received(rx, |parts| {
            assert_eq!(Method::POST, parts.method);
            assert_eq!("/frames", parts.uri.path());
        })
        .await;

        assert_eq!(num_lines, output_lines.len());
        assert_eq!(input_lines, output_lines);
    }

    #[tokio::test]
    async fn retries_on_temporary_error() {
        const NUM_LINES: usize = 1000;
        const NUM_FAILURES: usize = 2;

        let (in_addr, sink) = build_sink("").await;

        let counter = Arc::new(atomic::AtomicUsize::new(0));
        let in_counter = Arc::clone(&counter);
        let (rx, trigger, server) = build_test_server_generic(in_addr, move || {
            let count = in_counter.fetch_add(1, atomic::Ordering::Relaxed);
            if count < NUM_FAILURES {
                // Send a temporary error for the first two responses
                Response::builder()
                    .status(StatusCode::INTERNAL_SERVER_ERROR)
                    .body(Body::empty())
                    .unwrap_or_else(|_| unreachable!())
            } else {
                Response::new(Body::empty())
            }
        });

        let (batch, mut receiver) = BatchNotifier::new_with_receiver();
        let (input_lines, events) = random_lines_with_stream(100, NUM_LINES, Some(batch));
        let pump = sink.run(events);

        tokio::spawn(server);

        pump.await.unwrap();
        drop(trigger);

        assert_eq!(receiver.try_recv(), Ok(BatchStatus::Delivered));

        let output_lines = get_received(rx, |parts| {
            assert_eq!(Method::POST, parts.method);
            assert_eq!("/frames", parts.uri.path());
        })
        .await;

        let tries = counter.load(atomic::Ordering::Relaxed);
        assert!(tries > NUM_FAILURES);
        assert_eq!(NUM_LINES, output_lines.len());
        assert_eq!(input_lines, output_lines);
    }

    #[tokio::test]
    async fn fails_on_permanent_error() {
        let num_lines = 1000;

        let (in_addr, sink) = build_sink("").await;

        let (rx, trigger, server) = build_test_server_status(in_addr, StatusCode::FORBIDDEN);

        let (batch, mut receiver) = BatchNotifier::new_with_receiver();
        let (_input_lines, events) = random_lines_with_stream(100, num_lines, Some(batch));
        let pump = sink.run(events);

        tokio::spawn(server);

        pump.await.unwrap();
        drop(trigger);

        assert_eq!(receiver.try_recv(), Ok(BatchStatus::Rejected));

        let output_lines = get_received(rx, |_| unreachable!("There should be no lines")).await;
        assert!(output_lines.is_empty());
    }

    #[tokio::test]
    async fn json_compression() {
        let num_lines = 1000;

        let in_addr = next_addr();

        let config = r#"
        uri = "http://$IN_ADDR/frames"
        compression = "gzip"
        encoding = "json"

        [auth]
        strategy = "basic"
        user = "waldo"
        password = "hunter2"
    "#
        .replace("$IN_ADDR", &in_addr.to_string());
        let config: HttpSinkConfig = toml::from_str(&config).unwrap();

        let cx = SinkContext::new_test();

        let (sink, _) = config.build(cx).await.unwrap();
        let (rx, trigger, server) = build_test_server(in_addr);

        let (batch, mut receiver) = BatchNotifier::new_with_receiver();
        let (input_lines, events) = random_lines_with_stream(100, num_lines, Some(batch));
        let pump = sink.run(events);

        tokio::spawn(server);

        pump.await.unwrap();
        drop(trigger);

        assert_eq!(receiver.try_recv(), Ok(BatchStatus::Delivered));

        let output_lines = rx
            .flat_map(|(parts, body)| {
                assert_eq!(Method::POST, parts.method);
                assert_eq!("/frames", parts.uri.path());
                assert_eq!(
                    Some(Authorization::basic("waldo", "hunter2")),
                    parts.headers.typed_get()
                );

                let lines: Vec<serde_json::Value> =
                    serde_json::from_reader(MultiGzDecoder::new(body.reader())).unwrap();
                stream::iter(lines)
            })
            .map(|line| line.get("message").unwrap().as_str().unwrap().to_owned())
            .collect::<Vec<_>>()
            .await;

        assert_eq!(num_lines, output_lines.len());
        assert_eq!(input_lines, output_lines);
    }

    async fn get_received(
        rx: mpsc::Receiver<(Parts, Bytes)>,
        assert_parts: impl Fn(Parts),
    ) -> Vec<String> {
        rx.flat_map(|(parts, body)| {
            assert_parts(parts);
            stream::iter(BufReader::new(MultiGzDecoder::new(body.reader())).lines())
        })
        .map(Result::unwrap)
        .map(|line| {
            let val: serde_json::Value = serde_json::from_str(&line).unwrap();
            val.get("message").unwrap().as_str().unwrap().to_owned()
        })
        .collect::<Vec<_>>()
        .await
    }

    async fn run_sink(extra_config: &str, assert_parts: impl Fn(http::request::Parts)) {
        let num_lines = 1000;

        let (in_addr, sink) = build_sink(extra_config).await;

        let (rx, trigger, server) = build_test_server(in_addr);
        tokio::spawn(server);

        let (batch, mut receiver) = BatchNotifier::new_with_receiver();
        let (input_lines, events) = random_lines_with_stream(100, num_lines, Some(batch));
        components::run_sink(sink, events, &HTTP_SINK_TAGS).await;
        drop(trigger);

        assert_eq!(receiver.try_recv(), Ok(BatchStatus::Delivered));

        let output_lines = get_received(rx, assert_parts).await;

        assert_eq!(num_lines, output_lines.len());
        assert_eq!(input_lines, output_lines);
    }

    async fn build_sink(extra_config: &str) -> (std::net::SocketAddr, crate::sinks::VectorSink) {
        let in_addr = next_addr();
        let config = format!(
            r#"
                uri = "http://{addr}/frames"
                compression = "gzip"
                encoding = "ndjson"
                {extras}
            "#,
            addr = in_addr,
            extras = extra_config
        );
        let config: HttpSinkConfig = toml::from_str(&config).unwrap();

        let cx = SinkContext::new_test();

        let (sink, _) = config.build(cx).await.unwrap();
        (in_addr, sink)
    }
}
