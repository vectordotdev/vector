//! Configuration for the `http` sink.

use http::{header::AUTHORIZATION, HeaderName, HeaderValue, Method, Request, StatusCode};
use hyper::Body;
use indexmap::IndexMap;
use vector_lib::codecs::{
    encoding::{Framer, Serializer},
    CharacterDelimitedEncoder,
};
use crate::http::{HttpClientAuthorizationConfig};

use crate::{
    codecs::{EncodingConfigWithFraming, SinkType},
    http::{Auth, HttpClient, MaybeAuth},
    sinks::{
        prelude::*,
        util::{
            http::{http_response_retry_logic, HttpService, RequestConfig},
            RealtimeSizeBasedDefaultBatchSettings, UriSerde,
        },
    },
};

use super::{
    encoder::HttpEncoder, request_builder::HttpRequestBuilder, service::HttpSinkRequestBuilder,
    sink::HttpSink,
};

const CONTENT_TYPE_TEXT: &str = "text/plain";
const CONTENT_TYPE_NDJSON: &str = "application/x-ndjson";
const CONTENT_TYPE_JSON: &str = "application/json";

/// Configuration for the `http` sink.
#[configurable_component(sink("http", "Deliver observability event data to an HTTP server."))]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct HttpSinkConfig {
    /// The full URI to make HTTP requests to.
    ///
    /// This should include the protocol and host, but can also include the port, path, and any other valid part of a URI.
    #[configurable(metadata(docs::examples = "https://10.22.212.22:9000/endpoint"))]
    pub uri: UriSerde,

    /// The HTTP method to use when making the request.
    #[serde(default)]
    pub method: HttpMethod,

    #[configurable(derived)]
    pub auth: Option<Auth>,

    /// A list of custom headers to add to each request.
    #[configurable(deprecated = "This option has been deprecated, use `request.headers` instead.")]
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
    pub http_client_authorization_strategy: Option<HttpClientAuthorizationConfig>,

    #[configurable(derived)]
    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::is_default"
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
        Ok(HttpClient::new_asdasd(tls, cx.proxy(), self.http_client_authorization_strategy.clone())?)
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

        let encoder = self.build_encoder()?;
        let transformer = self.encoding.transformer();

        let mut request = self.request.clone();
        request.add_old_option(self.headers.clone());

        let headers = validate_headers(&request.headers, self.auth.is_some())?;

        let (payload_prefix, payload_suffix) =
            validate_payload_wrapper(&self.payload_prefix, &self.payload_suffix, &encoder)?;

        let client = self.build_http_client(&cx)?;

        let healthcheck = match cx.healthcheck.uri {
            Some(healthcheck_uri) => {
                healthcheck(healthcheck_uri, self.auth.clone(), client.clone()).boxed()
            }
            None => future::ok(()).boxed(),
        };

        let content_type = {
            use Framer::*;
            use Serializer::*;
            match (encoder.serializer(), encoder.framer()) {
                (RawMessage(_) | Text(_), _) => Some(CONTENT_TYPE_TEXT.to_owned()),
                (Json(_), NewlineDelimited(_)) => Some(CONTENT_TYPE_NDJSON.to_owned()),
                (Json(_), CharacterDelimited(CharacterDelimitedEncoder { delimiter: b',' })) => {
                    Some(CONTENT_TYPE_JSON.to_owned())
                }
                _ => None,
            }
        };

        let request_builder = HttpRequestBuilder {
            encoder: HttpEncoder::new(encoder, transformer, payload_prefix, payload_suffix),
            compression: self.compression,
        };

        let content_encoding = self.compression.is_compressed().then(|| {
            self.compression
                .content_encoding()
                .expect("Encoding should be specified for compression.")
                .to_string()
        });

        let http_sink_request_builder = HttpSinkRequestBuilder::new(
            self.uri.with_default_parts(),
            self.method,
            self.auth.choose_one(&self.uri.auth)?,
            headers,
            content_type,
            content_encoding,
        );

        let service = HttpService::new(client, http_sink_request_builder);

        let request_limits = self.request.tower.into_settings();

        let service = ServiceBuilder::new()
            .settings(request_limits, http_response_retry_logic())
            .service(service);

        let sink = HttpSink::new(service, batch_settings, request_builder);

        Ok((VectorSink::from_event_streamsink(sink), healthcheck))
    }

    fn input(&self) -> Input {
        Input::new(self.encoding.config().1.input_type())
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.acknowledgements
    }
}

#[cfg(test)]
mod tests {
    use vector_lib::codecs::encoding::format::JsonSerializerOptions;

    use super::*;
    use crate::components::validation::prelude::*;

    impl ValidatableComponent for HttpSinkConfig {
        fn validation_configuration() -> ValidationConfiguration {
            use std::str::FromStr;
            use vector_lib::codecs::{JsonSerializerConfig, MetricTagValues};
            use vector_lib::config::LogNamespace;

            let config = HttpSinkConfig {
                uri: UriSerde::from_str("http://127.0.0.1:9000/endpoint")
                    .expect("should never fail to parse"),
                method: HttpMethod::Post,
                encoding: EncodingConfigWithFraming::new(
                    None,
                    JsonSerializerConfig::new(
                        MetricTagValues::Full,
                        JsonSerializerOptions::default(),
                    )
                    .into(),
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
                http_client_authorization_strategy: None,
            };

            let external_resource = ExternalResource::new(
                ResourceDirection::Push,
                HttpResourceConfig::from_parts(config.uri.uri.clone(), Some(config.method.into())),
                config.encoding.clone(),
            );

            ValidationConfiguration::from_sink(
                Self::NAME,
                LogNamespace::Legacy,
                vec![ComponentTestCaseConfig::from_sink(
                    config,
                    None,
                    Some(external_resource),
                )],
            )
        }
    }

    register_validatable_component!(HttpSinkConfig);
}
