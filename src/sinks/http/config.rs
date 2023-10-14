//! Configuration for the `http` sink.

use codecs::{
    encoding::{Framer, Serializer},
    CharacterDelimitedEncoder,
};
use http::{
    header::{AUTHORIZATION, CONTENT_ENCODING, CONTENT_TYPE},
    HeaderName, HeaderValue, Method, Request, StatusCode,
};
use hyper::Body;
use indexmap::IndexMap;

use crate::{
    codecs::{EncodingConfigWithFraming, SinkType},
    http::{Auth, HttpClient, MaybeAuth},
    sinks::{
        prelude::*,
        util::{
            http::{
                http_response_retry_logic, GenericEventInputSplitter, HttpRequestBuilder,
                HttpService, RequestBlueprint, RequestConfig,
            },
            RealtimeSizeBasedDefaultBatchSettings, UriSerde,
        },
    },
};

use super::{encoder::HttpEncoder, sink::HttpSink};

const CONTENT_TYPE_TEXT: &str = "text/plain";
const CONTENT_TYPE_NDJSON: &str = "application/x-ndjson";
const CONTENT_TYPE_JSON: &str = "application/json";

/// Configuration for the `http` sink.
#[configurable_component(sink("http", "Deliver observability event data to an HTTP server."))]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]
pub(super) struct HttpSinkConfig {
    /// The full URI to make HTTP requests to.
    ///
    /// This should include the protocol and host, but can also include the port, path, and any other valid part of a URI.
    #[configurable(metadata(docs::examples = "https://10.22.212.22:9000/endpoint"))]
    pub(super) uri: UriSerde,

    /// The HTTP method to use when making the request.
    #[serde(default)]
    pub(super) method: HttpMethod,

    #[configurable(derived)]
    pub(super) auth: Option<Auth>,

    /// A list of custom headers to add to each request.
    #[configurable(deprecated = "This option has been deprecated, use `request.headers` instead.")]
    #[configurable(metadata(
        docs::additional_props_description = "An HTTP request header and it's value."
    ))]
    pub(super) headers: Option<IndexMap<String, String>>,

    #[configurable(derived)]
    #[serde(default)]
    pub(super) compression: Compression,

    #[serde(flatten)]
    pub(super) encoding: EncodingConfigWithFraming,

    /// A string to prefix the payload with.
    ///
    /// This option is ignored if the encoding is not character delimited JSON.
    ///
    /// If specified, the `payload_suffix` must also be specified and together they must produce a valid JSON object.
    #[configurable(metadata(docs::examples = "{\"data\":"))]
    #[serde(default)]
    pub(super) payload_prefix: String,

    /// A string to suffix the payload with.
    ///
    /// This option is ignored if the encoding is not character delimited JSON.
    ///
    /// If specified, the `payload_prefix` must also be specified and together they must produce a valid JSON object.
    #[configurable(metadata(docs::examples = "}"))]
    #[serde(default)]
    pub(super) payload_suffix: String,

    #[configurable(derived)]
    #[serde(default)]
    pub(super) batch: BatchConfig<RealtimeSizeBasedDefaultBatchSettings>,

    #[configurable(derived)]
    #[serde(default)]
    pub(super) request: RequestConfig,

    #[configurable(derived)]
    pub(super) tls: Option<TlsConfig>,

    #[configurable(derived)]
    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::skip_serializing_if_default"
    )]
    pub(super) acknowledgements: AcknowledgementsConfig,
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
pub(super) enum HttpMethod {
    /// GET.
    Get,

    /// HEAD.
    Head,

    /// POST.
    #[derivative(Default)]
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

impl HttpSinkConfig {
    fn build_http_client(&self, cx: &SinkContext) -> crate::Result<HttpClient> {
        let tls = TlsSettings::from_options(&self.tls)?;
        Ok(HttpClient::new(tls, cx.proxy())?)
    }

    pub(super) fn build_encoder(&self) -> crate::Result<Encoder<Framer>> {
        let (framer, serializer) = self.encoding.build(SinkType::MessageBased)?;
        Ok(Encoder::<Framer>::new(framer, serializer))
    }
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
        status => Err(HealthcheckError::UnexpectedStatus { status }.into()),
    }
}

pub(super) fn validate_headers(
    headers: &IndexMap<String, String>,
    configures_auth: bool,
) -> crate::Result<IndexMap<HeaderName, HeaderValue>> {
    let headers = crate::sinks::util::http::validate_headers(headers)?;

    for name in headers.keys() {
        if configures_auth && name == AUTHORIZATION {
            return Err("Authorization header can not be used with defined auth options".into());
        }
    }

    Ok(headers)
}

pub(super) fn validate_payload_wrapper(
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

#[async_trait]
#[typetag::serde(name = "http")]
impl SinkConfig for HttpSinkConfig {
    async fn build(&self, cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        let batch_settings = self.batch.validate()?.into_batcher_settings()?;

        let mut request = self.request.clone();
        request.add_old_option(self.headers.clone());

        let headers = validate_headers(&request.headers, self.auth.is_some())?;

        let encoder = self.build_encoder()?;
        let transformer = self.encoding.transformer();
        let (payload_prefix, payload_suffix) =
            validate_payload_wrapper(&self.payload_prefix, &self.payload_suffix, &encoder)?;

        let maybe_content_type = match (encoder.serializer(), encoder.framer()) {
            (Serializer::RawMessage(_) | Serializer::Text(_), _) => {
                Some(CONTENT_TYPE_TEXT.to_owned())
            }
            (Serializer::Json(_), Framer::NewlineDelimited(_)) => {
                Some(CONTENT_TYPE_NDJSON.to_owned())
            }
            (
                Serializer::Json(_),
                Framer::CharacterDelimited(CharacterDelimitedEncoder { delimiter: b',' }),
            ) => Some(CONTENT_TYPE_JSON.to_owned()),
            _ => None,
        };

        let maybe_content_encoding = self.compression.is_compressed().then(|| {
            self.compression
                .content_encoding()
                .expect("Encoding should be specified for compression.")
                .to_string()
        });

        let request_uri = self.uri.uri.clone();
        let request_blueprint = RequestBlueprint::from_uri(request_uri)
            .with_method(self.method.into())
            .add_headers(headers)?
            .add_header_maybe(CONTENT_TYPE, maybe_content_type)?
            .add_header_maybe(CONTENT_ENCODING, maybe_content_encoding)?
            .add_auth_maybe(self.auth.choose_one(&self.uri.auth)?);

        let http_encoder = HttpEncoder::new(encoder, transformer, payload_prefix, payload_suffix);
        let request_builder = HttpRequestBuilder::from_blueprint(request_blueprint)
            .with_input_splitter::<GenericEventInputSplitter>()
            .with_encoder(http_encoder);

        let http_client = self.build_http_client(&cx)?;
        let http_service = HttpService::new(http_client.clone());

        let request_limits = self.request.tower.unwrap_with(&Default::default());

        let service = ServiceBuilder::new()
            .settings(request_limits, http_response_retry_logic())
            .service(http_service);

        let sink = HttpSink::new(service, batch_settings, request_builder);

        let healthcheck = match cx.healthcheck.uri {
            Some(healthcheck_uri) => {
                healthcheck(healthcheck_uri, self.auth.clone(), http_client).boxed()
            }
            None => future::ok(()).boxed(),
        };

        Ok((VectorSink::from_event_streamsink(sink), healthcheck))
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
