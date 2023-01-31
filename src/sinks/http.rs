use std::io::Write;

use bytes::{BufMut, Bytes, BytesMut};
use codecs::encoding::{CharacterDelimitedEncoder, Framer, Serializer};
use flate2::write::{GzEncoder, ZlibEncoder};
use futures::{future, FutureExt, SinkExt};
use http::{
    header::{HeaderName, HeaderValue, AUTHORIZATION},
    Method, Request, StatusCode, Uri,
};
use hyper::Body;
use indexmap::IndexMap;
use tokio_util::codec::Encoder as _;
use vector_config::{configurable_component, NamedComponent};

use crate::{
    codecs::{Encoder, EncodingConfigWithFraming, SinkType, Transformer},
    components::validation::*,
    config::{AcknowledgementsConfig, GenerateConfig, Input, SinkConfig, SinkContext},
    event::Event,
    http::{Auth, HttpClient, MaybeAuth},
    register_validatable_component,
    sinks::util::{
        self,
        http::{BatchedHttpSink, HttpEventEncoder, RequestConfig},
        BatchConfig, Buffer, Compression, RealtimeSizeBasedDefaultBatchSettings,
        TowerRequestConfig, UriSerde,
    },
    tls::{TlsConfig, TlsSettings},
};

/// Configuration for the `http` sink.
#[configurable_component(sink("http"))]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct HttpSinkConfig {
    /// The full URI to make HTTP requests to.
    ///
    /// This should include the protocol and host, but can also include the port, path, and any other valid part of a URI.
    #[configurable(metadata(docs::examples = "https://10.22.212.22:9000/endpoint"))]
    pub uri: UriSerde,

    /// The HTTP method to use when making the request.
    #[serde(default = "default_http_method")]
    pub method: HttpMethod,

    #[configurable(derived)]
    pub auth: Option<Auth>,

    /// A list of custom headers to add to each request.
    #[configurable(deprecated)]
    #[configurable(metadata(
        docs::additional_props_description = "An HTTP request header and it's value."
    ))]
    pub headers: Option<IndexMap<String, String>>,

    #[configurable(derived)]
    #[serde(default)]
    pub compression: Compression,

    #[serde(flatten)]
    pub encoding: EncodingConfigWithFraming,

    /// A string to prefix the payload with.
    ///
    /// This option is ignored if the encoding is not character delimited JSON.
    ///
    /// If specified, the `payload_suffix` must also be specified and together they must produce a valid JSON object.
    #[configurable(metadata(docs::examples = "{\"data\":"))]
    #[serde(default)]
    pub payload_prefix: String,

    /// A string to suffix the payload with.
    ///
    /// This option is ignored if the encoding is not character delimited JSON.
    ///
    /// If specified, the `payload_prefix` must also be specified and together they must produce a valid JSON object.
    #[configurable(metadata(docs::examples = "}"))]
    #[serde(default)]
    pub payload_suffix: String,

    #[configurable(derived)]
    #[serde(default)]
    pub batch: BatchConfig<RealtimeSizeBasedDefaultBatchSettings>,

    #[configurable(derived)]
    #[serde(default)]
    pub request: RequestConfig,

    #[configurable(derived)]
    pub tls: Option<TlsConfig>,

    #[configurable(derived)]
    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::skip_serializing_if_default"
    )]
    pub acknowledgements: AcknowledgementsConfig,
}

/// HTTP method.
///
/// A subset of the HTTP methods described in [RFC 9110, section 9.1][rfc9110] are supported.
///
/// [rfc9110]: https://datatracker.ietf.org/doc/html/rfc9110#section-9.1
#[configurable_component]
#[derive(Clone, Copy, Debug, Derivative, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
#[derivative(Default)]
pub enum HttpMethod {
    /// GET.
    #[derivative(Default)]
    Get,

    /// HEAD.
    Head,

    /// POST.
    Post,

    /// PUT.
    Put,

    /// DELETE.
    Delete,

    /// OPTIONS.
    Options,

    /// TRACE.
    Trace,

    /// PATCH.
    Patch,
}

impl From<HttpMethod> for Method {
    fn from(http_method: HttpMethod) -> Self {
        match http_method {
            HttpMethod::Head => Self::HEAD,
            HttpMethod::Get => Self::GET,
            HttpMethod::Post => Self::POST,
            HttpMethod::Put => Self::PUT,
            HttpMethod::Patch => Self::PATCH,
            HttpMethod::Delete => Self::DELETE,
            HttpMethod::Options => Self::OPTIONS,
            HttpMethod::Trace => Self::TRACE,
        }
    }
}

const fn default_http_method() -> HttpMethod {
    HttpMethod::Get
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
    pub method: HttpMethod,
    pub auth: Option<Auth>,
    pub payload_prefix: String,
    pub payload_suffix: String,
    pub compression: Compression,
    pub transformer: Transformer,
    pub encoder: Encoder<Framer>,
    pub batch: BatchConfig<RealtimeSizeBasedDefaultBatchSettings>,
    pub tower: TowerRequestConfig,
    pub headers: IndexMap<HeaderName, HeaderValue>,
}

#[cfg(test)]
fn default_sink(encoding: EncodingConfigWithFraming) -> HttpSink {
    let (framing, serializer) = encoding.build(SinkType::MessageBased).unwrap();
    let encoder = Encoder::<Framer>::new(framing, serializer);

    HttpSink {
        uri: Default::default(),
        method: Default::default(),
        auth: Default::default(),
        compression: Default::default(),
        transformer: Default::default(),
        encoder,
        payload_prefix: Default::default(),
        payload_suffix: Default::default(),
        batch: Default::default(),
        tower: Default::default(),
        headers: Default::default(),
    }
}

#[async_trait::async_trait]
impl SinkConfig for HttpSinkConfig {
    async fn build(
        &self,
        cx: SinkContext,
    ) -> crate::Result<(super::VectorSink, super::Healthcheck)> {
        let client = self.build_http_client(&cx)?;

        let healthcheck = match cx.healthcheck.uri {
            Some(healthcheck_uri) => {
                healthcheck(healthcheck_uri, self.auth.clone(), client.clone()).boxed()
            }
            None => future::ok(()).boxed(),
        };

        let mut request = self.request.clone();
        request.add_old_option(self.headers.clone());
        let headers = validate_headers(&request.headers, self.auth.is_some())?;

        let (framer, serializer) = self.encoding.build(SinkType::MessageBased)?;
        let encoder = Encoder::<Framer>::new(framer, serializer);

        let (payload_prefix, payload_suffix) =
            validate_payload_wrapper(&self.payload_prefix, &self.payload_suffix, &encoder)?;

        let sink = HttpSink {
            uri: self.uri.with_default_parts(),
            method: self.method,
            auth: self.auth.choose_one(&self.uri.auth)?,
            compression: self.compression,
            transformer: self.encoding.transformer(),
            encoder,
            batch: self.batch,
            tower: request.tower,
            headers,
            payload_prefix,
            payload_suffix,
        };

        let request = sink.tower.unwrap_with(&TowerRequestConfig::default());

        let batch = sink.batch.into_batch_settings()?;
        let sink = BatchedHttpSink::new(
            sink,
            Buffer::new(batch.size, Compression::None),
            request,
            batch.timeout,
            client,
        )
        .sink_map_err(|error| error!(message = "Fatal HTTP sink error.", %error));

        let sink = super::VectorSink::from_event_sink(sink);

        Ok((sink, healthcheck))
    }

    fn input(&self) -> Input {
        Input::new(self.encoding.config().1.input_type())
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.acknowledgements
    }
}

impl ValidatableComponent for HttpSinkConfig {
    fn validation_configuration() -> ValidationConfiguration {
        use codecs::{JsonSerializerConfig, MetricTagValues};
        use std::str::FromStr;

        let config = Self {
            uri: UriSerde::from_str("http://127.0.0.1:9000/endpoint")
                .expect("should never fail to parse"),
            method: HttpMethod::Post,
            encoding: EncodingConfigWithFraming::new(
                None,
                JsonSerializerConfig::new(MetricTagValues::Full).into(),
                Transformer::default(),
            ),
            auth: None,
            headers: None,
            compression: Compression::default(),
            batch: BatchConfig::default(),
            request: RequestConfig::default(),
            tls: None,
            acknowledgements: AcknowledgementsConfig::default(),
            payload_prefix: String::new(),
            payload_suffix: String::new(),
        };

        let external_resource = ExternalResource::new(
            ResourceDirection::Push,
            HttpResourceConfig::from_parts(config.uri.uri.clone(), Some(config.method.into())),
            config.encoding.clone(),
        );

        ValidationConfiguration::from_sink(Self::NAME, config, Some(external_resource))
    }
}

register_validatable_component!(HttpSinkConfig);

pub struct HttpSinkEventEncoder {
    encoder: Encoder<Framer>,
    transformer: Transformer,
}

impl HttpEventEncoder<BytesMut> for HttpSinkEventEncoder {
    fn encode_event(&mut self, mut event: Event) -> Option<BytesMut> {
        self.transformer.transform(&mut event);

        let mut body = BytesMut::new();
        self.encoder.encode(event, &mut body).ok()?;

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
        let method: Method = self.method.into();
        let uri: Uri = self.uri.uri.clone();

        let content_type = {
            use Framer::*;
            use Serializer::*;
            match (self.encoder.serializer(), self.encoder.framer()) {
                (RawMessage(_) | Text(_), _) => Some("text/plain"),
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
                    body.put(self.payload_prefix.as_bytes());
                    body.put_u8(b'[');
                    if !message.is_empty() {
                        body.unsplit(message);
                        // remove trailing comma from last record
                        body.truncate(body.len() - 1);
                    }
                    body.put_u8(b']');
                    body.put(self.payload_suffix.as_bytes());
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
                let mut w = GzEncoder::new(buffer.writer(), level.as_flate2());
                w.write_all(&body).expect("Writing to Vec can't fail");
                body = w.finish().expect("Writing to Vec can't fail").into_inner();
            }
            Compression::Zlib(level) => {
                builder = builder.header("Content-Encoding", "deflate");

                let buffer = BytesMut::new();
                let mut w = ZlibEncoder::new(buffer.writer(), level.as_flate2());
                w.write_all(&body).expect("Writing to Vec can't fail");
                body = w.finish().expect("Writing to Vec can't fail").into_inner();
            }
            Compression::None => {}
        }

        let headers = builder
            .headers_mut()
            // The request builder should not have errors at this point, and if it did it would fail in the call to `body()` also.
            .expect("Failed to access headers in http::Request builder- builder has errors.");
        for (header, value) in self.headers.iter() {
            headers.insert(header, value.clone());
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

fn validate_headers(
    headers: &IndexMap<String, String>,
    configures_auth: bool,
) -> crate::Result<IndexMap<HeaderName, HeaderValue>> {
    let headers = util::http::validate_headers(headers)?;

    for name in headers.keys() {
        if configures_auth && name == AUTHORIZATION {
            return Err("Authorization header can not be used with defined auth options".into());
        }
    }

    Ok(headers)
}

fn validate_payload_wrapper(
    payload_prefix: &str,
    payload_suffix: &str,
    encoder: &Encoder<Framer>,
) -> crate::Result<(String, String)> {
    let payload = [payload_prefix, "{}", payload_suffix].join("");
    match (
        encoder.serializer(),
        encoder.framer(),
        serde_json::from_str::<serde_json::Value>(&payload),
    ) {
        (
            Serializer::Json(_),
            Framer::CharacterDelimited(CharacterDelimitedEncoder { delimiter: b',' }),
            Err(_),
        ) => Err("Payload prefix and suffix wrapper must produce a valid JSON object.".into()),
        _ => Ok((payload_prefix.to_owned(), payload_suffix.to_owned())),
    }
}

#[cfg(test)]
mod tests {
    use std::{
        io::{BufRead, BufReader},
        sync::{atomic, Arc},
    };

    use bytes::{Buf, Bytes};
    use codecs::{
        encoding::FramingConfig, JsonSerializerConfig, NewlineDelimitedEncoderConfig,
        TextSerializerConfig,
    };
    use flate2::read::MultiGzDecoder;
    use futures::{channel::mpsc, stream, StreamExt};
    use headers::{Authorization, HeaderMapExt};
    use http::request::Parts;
    use hyper::{Method, Response, StatusCode};
    use serde::Deserialize;
    use vector_core::event::{BatchNotifier, BatchStatus, LogEvent};

    use super::*;
    use crate::{
        assert_downcast_matches,
        config::SinkContext,
        sinks::util::{
            http::{HeaderValidationError, HttpSink},
            test::{build_test_server, build_test_server_generic, build_test_server_status},
        },
        test_util::{
            components,
            components::{COMPONENT_ERROR_TAGS, HTTP_SINK_TAGS},
            next_addr, random_lines_with_stream,
        },
    };

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<HttpSinkConfig>();
    }

    #[test]
    fn http_encode_event_text() {
        let event = Event::Log(LogEvent::from("hello world"));

        let sink = default_sink((None::<FramingConfig>, TextSerializerConfig::default()).into());
        let mut encoder = sink.build_encoder();
        let bytes = encoder.encode_event(event).unwrap();

        assert_eq!(bytes, Vec::from("hello world\n"));
    }

    #[test]
    fn http_encode_event_ndjson() {
        let event = Event::Log(LogEvent::from("hello world"));

        let sink = default_sink(
            (
                Some(NewlineDelimitedEncoderConfig::new()),
                JsonSerializerConfig::default(),
            )
                .into(),
        );
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
        encoding.codec = "text"
        [request.headers]
        Auth = "token:thing_and-stuff"
        X-Custom-Nonsense = "_%_{}_-_&_._`_|_~_!_#_&_$_"
        "#;
        let config: HttpSinkConfig = toml::from_str(config).unwrap();

        assert!(super::validate_headers(&config.request.headers, false).is_ok());
    }

    #[test]
    fn http_catches_bad_header_names() {
        let config = r#"
        uri = "http://$IN_ADDR/frames"
        encoding.codec = "text"
        [request.headers]
        "\u0001" = "bad"
        "#;
        let config: HttpSinkConfig = toml::from_str(config).unwrap();

        assert_downcast_matches!(
            super::validate_headers(&config.request.headers, false).unwrap_err(),
            HeaderValidationError,
            HeaderValidationError::InvalidHeaderName { .. }
        );
    }

    #[test]
    fn http_validates_payload_prefix_and_suffix() {
        let config = r#"
        uri = "http://$IN_ADDR/"
        encoding.codec = "json"
        payload_prefix = '{"data":'
        payload_suffix = "}"
        "#;
        let config: HttpSinkConfig = toml::from_str(config).unwrap();
        let (framer, serializer) = config.encoding.build(SinkType::MessageBased).unwrap();
        let encoder = Encoder::<Framer>::new(framer, serializer);
        assert!(super::validate_payload_wrapper(
            &config.payload_prefix,
            &config.payload_suffix,
            &encoder
        )
        .is_ok());
    }

    #[test]
    fn http_validates_payload_prefix_and_suffix_fails_on_invalid_json() {
        let config = r#"
        uri = "http://$IN_ADDR/"
        encoding.codec = "json"
        payload_prefix = '{"data":'
        payload_suffix = ""
        "#;
        let config: HttpSinkConfig = toml::from_str(config).unwrap();
        let (framer, serializer) = config.encoding.build(SinkType::MessageBased).unwrap();
        let encoder = Encoder::<Framer>::new(framer, serializer);
        assert!(super::validate_payload_wrapper(
            &config.payload_prefix,
            &config.payload_suffix,
            &encoder
        )
        .is_err());
    }

    // TODO: Fix failure on GH Actions using macos-latest image.
    #[cfg(not(target_os = "macos"))]
    #[tokio::test]
    #[should_panic(expected = "Authorization header can not be used with defined auth options")]
    async fn http_headers_auth_conflict() {
        let config = r#"
        uri = "http://$IN_ADDR/"
        encoding.codec = "text"
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
            "post",
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
        [auth]
        strategy = "basic"
        user = "waldo"
        password = "hunter2"
    "#,
            "put",
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
            "post",
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
        components::assert_sink_compliance(&HTTP_SINK_TAGS, async {
            let num_lines = 10;

            let (in_addr, sink) = build_sink("", "post").await;

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
        })
        .await;
    }

    #[tokio::test]
    async fn retries_on_temporary_error() {
        components::assert_sink_compliance(&HTTP_SINK_TAGS, async {
            const NUM_LINES: usize = 1000;
            const NUM_FAILURES: usize = 2;

            let (in_addr, sink) = build_sink("", "post").await;

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
        })
        .await;
    }

    #[tokio::test]
    async fn fails_on_permanent_error() {
        components::assert_sink_error(&COMPONENT_ERROR_TAGS, async {
            let num_lines = 1000;

            let (in_addr, sink) = build_sink("", "post").await;

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
        })
        .await;
    }

    #[tokio::test]
    async fn json_compression() {
        components::assert_sink_compliance(&HTTP_SINK_TAGS, async {
            let num_lines = 1000;

            let in_addr = next_addr();

            let config = r#"
        uri = "http://$IN_ADDR/frames"
        compression = "gzip"
        encoding.codec = "json"
        method = "post"

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
        })
        .await;
    }

    #[tokio::test]
    async fn json_compression_with_payload_wrapper() {
        components::assert_sink_compliance(&HTTP_SINK_TAGS, async {
            let num_lines = 1000;

            let in_addr = next_addr();

            let config = r#"
        uri = "http://$IN_ADDR/frames"
        compression = "gzip"
        encoding.codec = "json"
        payload_prefix = '{"data":'
        payload_suffix = "}"
        method = "post"

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

                    let message: serde_json::Value =
                        serde_json::from_reader(MultiGzDecoder::new(body.reader())).unwrap();
                    let lines: Vec<serde_json::Value> =
                        message["data"].as_array().unwrap().to_vec();
                    stream::iter(lines)
                })
                .map(|line| line.get("message").unwrap().as_str().unwrap().to_owned())
                .collect::<Vec<_>>()
                .await;

            assert_eq!(num_lines, output_lines.len());
            assert_eq!(input_lines, output_lines);
        })
        .await;
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

    async fn run_sink(
        extra_config: &str,
        method: &str,
        assert_parts: impl Fn(http::request::Parts),
    ) {
        let num_lines = 1000;

        let (in_addr, sink) = build_sink(extra_config, method).await;

        let (rx, trigger, server) = build_test_server(in_addr);
        tokio::spawn(server);

        let (batch, mut receiver) = BatchNotifier::new_with_receiver();
        let (input_lines, events) = random_lines_with_stream(100, num_lines, Some(batch));
        components::run_and_assert_sink_compliance(sink, events, &HTTP_SINK_TAGS).await;
        drop(trigger);

        assert_eq!(receiver.try_recv(), Ok(BatchStatus::Delivered));

        let output_lines = get_received(rx, assert_parts).await;

        assert_eq!(num_lines, output_lines.len());
        assert_eq!(input_lines, output_lines);
    }

    async fn build_sink(
        extra_config: &str,
        method: &str,
    ) -> (std::net::SocketAddr, crate::sinks::VectorSink) {
        let in_addr = next_addr();

        let config = format!(
            r#"
                uri = "http://{addr}/frames"
                compression = "gzip"
                framing.method = "newline_delimited"
                encoding.codec = "json"
                method = "{method}"
                {extras}
            "#,
            addr = in_addr,
            extras = extra_config,
            method = method
        );
        let config: HttpSinkConfig = toml::from_str(&config).unwrap();

        let cx = SinkContext::new_test();

        let (sink, _) = config.build(cx).await.unwrap();
        (in_addr, sink)
    }
}
