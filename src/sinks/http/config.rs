//! Configuration for the `http` sink.

use std::{collections::BTreeMap, path::PathBuf};

#[cfg(feature = "aws-core")]
use aws_config::meta::region::ProvideRegion;
#[cfg(feature = "aws-core")]
use aws_types::region::Region;
use http::{HeaderName, HeaderValue, Method, Request, StatusCode, header::AUTHORIZATION};
use hyper::Body;
use vector_lib::codecs::{
    CharacterDelimitedEncoderConfig, LengthDelimitedEncoderConfig,
    encoding::{Framer, FramingConfig, SerializerConfig},
};
#[cfg(feature = "aws-core")]
use vector_lib::config::proxy::ProxyConfig;

use super::{
    encoder::HttpEncoder, request_builder::HttpRequestBuilder, service::HttpSinkRequestBuilder,
    sink::HttpSink,
};
#[cfg(feature = "aws-core")]
use crate::aws::AwsAuthentication;
#[cfg(feature = "aws-core")]
use crate::sinks::util::http::SigV4Config;
use crate::{
    codecs::{EncodingConfigWithFraming, SinkType},
    config::ValidatedSink,
    http::{Auth, HttpClient, MaybeAuth},
    sinks::{
        prelude::*,
        util::{
            RealtimeSizeBasedDefaultBatchSettings, TowerRequestSettings, UriSerde,
            http::{
                HttpService, OrderedHeaderName, RequestConfig, RetryStrategy,
                http_response_retry_logic,
            },
        },
    },
    template::{ConfinementConfig, UriTemplate},
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
    pub uri: UriTemplate,

    /// The HTTP method to use when making the request.
    #[serde(default)]
    pub method: HttpMethod,

    pub auth: Option<Auth>,

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

    #[serde(default)]
    pub batch: BatchConfig<RealtimeSizeBasedDefaultBatchSettings>,

    #[serde(default)]
    pub request: RequestConfig,

    pub tls: Option<TlsConfig>,

    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::is_default"
    )]
    pub acknowledgements: AcknowledgementsConfig,

    #[serde(default)]
    pub retry_strategy: RetryStrategy,

    #[serde(flatten)]
    pub confinement: ConfinementConfig,
}

/// HTTP method.
///
/// A subset of the HTTP methods described in [RFC 9110, section 9.1][rfc9110] are supported.
///
/// [rfc9110]: https://datatracker.ietf.org/doc/html/rfc9110#section-9.1
#[configurable_component]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum HttpMethod {
    /// GET.
    Get,

    /// HEAD.
    Head,

    /// POST.
    #[default]
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
        let tls = TlsSettings::from_options(self.tls.as_ref())?;
        Ok(HttpClient::new(tls, cx.proxy())?)
    }

    #[cfg(test)]
    pub(super) fn build_encoder(&self) -> crate::Result<Encoder<Framer>> {
        let (framer, serializer) = self.encoding.build(SinkType::MessageBased)?;
        Ok(Encoder::<Framer>::new(framer, serializer))
    }
}

impl GenerateConfig for HttpSinkConfig {
    fn generate_config() -> serde_json::Value {
        serde_yaml::from_str(indoc::indoc! {
            r#"uri: https://10.22.212.22:9000/endpoint
            encoding:
              codec: json"#,
        })
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
    headers: &BTreeMap<String, String>,
    configures_auth: bool,
) -> crate::Result<BTreeMap<OrderedHeaderName, HeaderValue>> {
    let headers = crate::sinks::util::http::validate_headers(headers)?;

    for name in headers.keys() {
        if configures_auth && name.inner() == AUTHORIZATION {
            return Err("Authorization header can not be used with defined auth options".into());
        }
    }

    Ok(headers)
}

/// Returns the effective framing configuration for the `http` sink (which is
/// message-based): the explicit framing if set, otherwise the default for the
/// serializer. This mirrors the framer selection in
/// `EncodingConfigWithFraming::build` without building the serializer (which
/// may read files for codecs such as protobuf).
fn effective_framer_config(encoding: &EncodingConfigWithFraming) -> FramingConfig {
    match encoding.config().0 {
        Some(framing) => framing.clone(),
        None => match encoding.config().1 {
            SerializerConfig::Json(_) => {
                FramingConfig::CharacterDelimited(CharacterDelimitedEncoderConfig::new(b','))
            }
            SerializerConfig::Avro { .. } | SerializerConfig::Native => {
                FramingConfig::LengthDelimited(LengthDelimitedEncoderConfig::default())
            }
            SerializerConfig::Gelf(_) => {
                FramingConfig::CharacterDelimited(CharacterDelimitedEncoderConfig::new(0))
            }
            SerializerConfig::Protobuf(_) => {
                FramingConfig::LengthDelimited(LengthDelimitedEncoderConfig::default())
            }
            SerializerConfig::Cef(_)
            | SerializerConfig::Csv(_)
            | SerializerConfig::Logfmt
            | SerializerConfig::NativeJson
            | SerializerConfig::RawMessage
            | SerializerConfig::Text(_) => FramingConfig::NewlineDelimited,
            #[cfg(feature = "codecs-syslog")]
            SerializerConfig::Syslog(_) => FramingConfig::NewlineDelimited,
            #[cfg(feature = "codecs-opentelemetry")]
            SerializerConfig::Otlp => FramingConfig::Bytes,
        },
    }
}

pub(super) fn validate_payload_wrapper(
    payload_prefix: &str,
    payload_suffix: &str,
    serializer: &SerializerConfig,
    framer: &FramingConfig,
) -> crate::Result<(String, String)> {
    let payload = [payload_prefix, "{}", payload_suffix].join("");
    match (
        serializer,
        framer,
        serde_json::from_str::<serde_json::Value>(&payload),
    ) {
        (SerializerConfig::Json(_), FramingConfig::CharacterDelimited(cfg), Err(_))
            if cfg.character_delimited.delimiter == b',' =>
        {
            Err("Payload prefix and suffix wrapper must produce a valid JSON object.".into())
        }
        _ => Ok((payload_prefix.to_owned(), payload_suffix.to_owned())),
    }
}

#[async_trait]
#[typetag::serde(name = "http")]
impl SinkConfig for HttpSinkConfig {
    fn confinement_config(&self) -> Option<&crate::template::ConfinementConfig> {
        Some(&self.confinement)
    }

    fn input(&self) -> Input {
        Input::new(self.encoding.config().1.input_type())
    }

    fn files_to_watch(&self) -> Vec<&PathBuf> {
        let mut files = Vec::new();
        if let Some(tls) = &self.tls {
            if let Some(crt_file) = &tls.crt_file {
                files.push(crt_file)
            }
            if let Some(key_file) = &tls.key_file {
                files.push(key_file)
            }
        };
        files
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.acknowledgements
    }
}

#[derive(Clone, Debug)]
pub struct ValidatedHttp {
    batch_settings: BatcherSettings,
    transformer: Transformer,
    template_headers: BTreeMap<String, Template>,
    payload_prefix: String,
    payload_suffix: String,
    content_type: Option<String>,
    content_encoding: Option<String>,
    converted_static_headers: BTreeMap<OrderedHeaderName, HeaderValue>,
    request_limits: TowerRequestSettings,
}

#[async_trait::async_trait]
impl ValidatedSink for HttpSinkConfig {
    type Validated = ValidatedHttp;

    fn validate(&self) -> crate::Result<ValidatedHttp> {
        let batch_settings = self.batch.validate()?.into_batcher_settings()?;

        let serializer_config = self.encoding.config().1;
        let framer_config = effective_framer_config(&self.encoding);
        let transformer = self.encoding.transformer();

        let request = self.request.clone();

        validate_headers(&request.headers, self.auth.is_some())?;
        let (static_headers, template_headers) = request.split_headers();

        // Pure confinement checks for the URI and templated headers. The actual
        // confinement (with the component name threaded from the
        // `opentelemetry`/`axiom` delegations) happens in `build_from_validated`;
        // running the checks here lets `vector validate --no-environment` catch
        // unconfined routing templates. Skipped under the full opt-out, where
        // `confine` only emits a warning.
        if !self
            .confinement
            .dangerously_allow_unconfined_template_resolution
        {
            self.uri
                .clone()
                .confine(&self.confinement, Self::NAME, "uri")?;
            for tpl in template_headers.values() {
                tpl.clone()
                    .confine(&self.confinement, Self::NAME, "request.headers")?;
            }
        }

        // `UriTemplate::default()` — produced by delegating sinks such as
        // `opentelemetry` before the user supplies a URI — yields an empty
        // template whose `is_static` is false, so `is_dynamic()` reports true
        // even though there is nothing to render. Reject the empty URI up
        // front rather than deferring a guaranteed per-request failure.
        if self.uri.is_empty() {
            return Err("uri must not be empty, e.g. `https://example.com/endpoint`"
                .to_string()
                .into());
        }

        // A static URI can be parsed and checked for embedded credentials up
        // front; dynamic URIs are only validated at render time.
        if !self.uri.is_dynamic() {
            let uri_serde: UriSerde = self.uri.get_ref().parse()?;
            self.auth.choose_one(&uri_serde.auth)?;
            if uri_serde.uri.scheme().is_none() || uri_serde.uri.authority().is_none() {
                return Err(format!(
                    "uri must include a scheme and host, e.g. `https://example.com/endpoint`; got `{}`",
                    self.uri.get_ref()
                )
                .into());
            }
        }

        let (payload_prefix, payload_suffix) = validate_payload_wrapper(
            &self.payload_prefix,
            &self.payload_suffix,
            serializer_config,
            &framer_config,
        )?;

        let content_type = {
            use FramingConfig::*;
            use SerializerConfig::*;
            match (serializer_config, &framer_config) {
                (RawMessage | Text(_), _) => Some(CONTENT_TYPE_TEXT.to_owned()),
                (Json(_), NewlineDelimited) => Some(CONTENT_TYPE_NDJSON.to_owned()),
                (Json(_), CharacterDelimited(cfg)) if cfg.character_delimited.delimiter == b',' => {
                    Some(CONTENT_TYPE_JSON.to_owned())
                }
                #[cfg(feature = "codecs-opentelemetry")]
                (Otlp, _) => Some("application/x-protobuf".to_owned()),
                _ => None,
            }
        };

        let content_encoding = self.compression.is_compressed().then(|| {
            self.compression
                .content_encoding()
                .expect("Encoding should be specified for compression.")
                .to_string()
        });

        let converted_static_headers = static_headers
            .into_iter()
            .map(|(name, value)| -> crate::Result<_> {
                let header_name =
                    HeaderName::from_bytes(name.as_bytes()).map(OrderedHeaderName::from)?;
                let header_value = HeaderValue::try_from(value)?;
                Ok((header_name, header_value))
            })
            .collect::<Result<BTreeMap<_, _>, _>>()?;

        let request_limits = self.request.tower.into_settings();

        Ok(ValidatedHttp {
            batch_settings,
            transformer,
            template_headers,
            payload_prefix,
            payload_suffix,
            content_type,
            content_encoding,
            converted_static_headers,
            request_limits,
        })
    }

    async fn build(
        &self,
        validated: &ValidatedHttp,
        cx: SinkContext,
    ) -> crate::Result<(VectorSink, Healthcheck)> {
        self.build_from_validated(validated, cx, Self::NAME).await
    }
}

impl HttpSinkConfig {
    /// Builds the sink from the validated state. Confinement of the URI and
    /// templated headers happens here (not in `validate`) because the
    /// `component_name` threaded from the `opentelemetry`/`axiom` delegations
    /// must appear in per-template security warnings.
    pub(crate) async fn build_from_validated(
        &self,
        validated: &ValidatedHttp,
        cx: SinkContext,
        component_name: &'static str,
    ) -> crate::Result<(VectorSink, Healthcheck)> {
        let ValidatedHttp {
            batch_settings,
            transformer,
            template_headers,
            payload_prefix,
            payload_suffix,
            content_type,
            content_encoding,
            converted_static_headers,
            request_limits,
        } = validated;

        let client = self.build_http_client(&cx)?;

        let healthcheck = match cx.healthcheck.uri {
            Some(healthcheck_uri) => {
                healthcheck(healthcheck_uri, self.auth.clone(), client.clone()).boxed()
            }
            None => future::ok(()).boxed(),
        };

        let (framer, serializer) = self.encoding.build(SinkType::MessageBased)?;
        let encoder = Encoder::<Framer>::new(framer, serializer);

        let request_builder = HttpRequestBuilder {
            encoder: HttpEncoder::new(
                encoder,
                transformer.clone(),
                payload_prefix.clone(),
                payload_suffix.clone(),
            ),
            compression: self.compression,
        };

        let http_sink_request_builder = HttpSinkRequestBuilder::new(
            self.method,
            self.auth.clone(),
            converted_static_headers.clone(),
            content_type.clone(),
            content_encoding.clone(),
        );

        let service = match &self.auth {
            #[cfg(feature = "aws-core")]
            Some(Auth::Aws { auth, service }) => {
                let default_region = crate::aws::region_provider(&ProxyConfig::default(), None)?
                    .region()
                    .await;
                let region = (match &auth {
                    AwsAuthentication::AccessKey { region, .. } => region.clone(),
                    AwsAuthentication::File { .. } => None,
                    AwsAuthentication::Role { region, .. } => region.clone(),
                    AwsAuthentication::Default { region, .. } => region.clone(),
                })
                .map_or(default_region, |r| Some(Region::new(r.to_string())))
                .expect("Region must be specified");

                HttpService::new_with_sig_v4(
                    client,
                    http_sink_request_builder,
                    SigV4Config {
                        shared_credentials_provider: auth
                            .credentials_provider(region.clone(), &ProxyConfig::default(), None)
                            .await?,
                        region: region.clone(),
                        service: service.clone(),
                    },
                )
            }
            _ => HttpService::new(client, http_sink_request_builder),
        };

        let service = ServiceBuilder::new()
            .settings(
                request_limits.clone(),
                http_response_retry_logic(self.retry_strategy.clone()),
            )
            .service(service);

        let uri = self
            .uri
            .clone()
            .confine(&self.confinement, component_name, "uri")?;

        // Confine every templated header value. Header-based routing
        // (e.g. `X-Scope-OrgID: "{{ tenant }}"`) is as steerable as URI
        // routing — an event that controls the header field picks the
        // destination tenant unless we confine the header template too.
        let template_headers = template_headers
            .clone()
            .into_iter()
            .map(|(name, tpl)| {
                tpl.confine(&self.confinement, component_name, "request.headers")
                    .map(|tpl| (name, tpl))
            })
            .collect::<crate::Result<BTreeMap<_, _>>>()?;

        let sink = HttpSink::new(
            service,
            uri,
            template_headers,
            *batch_settings,
            request_builder,
        );

        Ok((VectorSink::from_event_streamsink(sink), healthcheck))
    }
}

#[cfg(test)]
mod tests {
    use vector_lib::codecs::encoding::format::JsonSerializerOptions;

    use super::*;
    use crate::components::validation::prelude::*;
    use crate::template::{ConfinementConfig, UriTemplate};

    impl ValidatableComponent for HttpSinkConfig {
        fn validation_configuration() -> ValidationConfiguration {
            use std::str::FromStr;

            use vector_lib::{
                codecs::{JsonSerializerConfig, MetricTagValues},
                config::LogNamespace,
            };

            let endpoint = "http://127.0.0.1:9000/endpoint";
            let uri = UriSerde::from_str(endpoint).expect("should never fail to parse");

            let config = HttpSinkConfig {
                uri: UriTemplate::try_from(endpoint).expect("should never fail to parse"),
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
                compression: Compression::default(),
                batch: BatchConfig::default(),
                request: RequestConfig::default(),
                tls: None,
                acknowledgements: AcknowledgementsConfig::default(),
                payload_prefix: String::new(),
                payload_suffix: String::new(),
                retry_strategy: RetryStrategy::default(),
                confinement: ConfinementConfig::default(),
            };

            let external_resource = ExternalResource::new(
                ResourceDirection::Push,
                HttpResourceConfig::from_parts(uri.uri, Some(config.method.into())),
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

    #[test]
    fn validate_rejects_static_uri_with_auth_conflict() {
        use crate::config::ValidatedSink;
        let config: HttpSinkConfig = serde_yaml::from_str(
            r#"
            uri: "http://user:pass@localhost:9000/endpoint"
            auth:
              strategy: basic
              user: user
              password: pass
            encoding:
              codec: json
            "#,
        )
        .unwrap();
        assert!(
            config.validate().is_err(),
            "embedded credentials plus auth should fail validation"
        );
    }

    #[test]
    fn validate_accepts_static_uri() {
        use crate::config::ValidatedSink;
        let config: HttpSinkConfig = serde_yaml::from_str(
            r#"
            uri: "http://localhost:9000/endpoint"
            encoding:
              codec: json
            "#,
        )
        .unwrap();
        config.validate().expect("valid static uri should validate");
    }

    #[test]
    fn validate_accepts_dynamic_uri() {
        use crate::config::ValidatedSink;
        let config: HttpSinkConfig = serde_yaml::from_str(
            r#"
            uri: "http://example.com/{{ path }}"
            encoding:
              codec: json
            "#,
        )
        .unwrap();
        config
            .validate()
            .expect("dynamic uri validation is deferred to render time");
    }

    #[test]
    fn validate_rejects_empty_default_uri() {
        use crate::config::ValidatedSink;
        // `UriTemplate::default()` — produced by delegating sinks such as
        // `opentelemetry` before the user supplies a URI — is empty but reports
        // `is_dynamic() == true` (the derived default leaves `is_static` false),
        // so it must be rejected explicitly rather than deferred as a dynamic
        // template that can never render.
        let mut config: HttpSinkConfig = serde_yaml::from_str(
            r#"
            uri: "http://localhost:9000/endpoint"
            encoding:
              codec: json
            "#,
        )
        .unwrap();
        config.uri = UriTemplate::default();
        assert!(
            config.validate().is_err(),
            "empty default uri should fail validation"
        );
    }

    #[test]
    fn validate_rejects_relative_static_uri() {
        use crate::config::ValidatedSink;
        let config: HttpSinkConfig = serde_yaml::from_str(
            r#"
            uri: "/ingest"
            encoding:
              codec: json
            "#,
        )
        .unwrap();
        assert!(
            config.validate().is_err(),
            "relative static uri should fail validation"
        );
    }

    #[test]
    fn confinement_rejects_unconfined_uri() {
        let template: UriTemplate = "{{ endpoint }}".try_into().unwrap();
        let err = template
            .confine(&ConfinementConfig::default(), "http", "uri")
            .unwrap_err();
        assert!(
            err.to_string().contains("no literal string prefix"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn confinement_opt_out_allows_unconfined_uri() {
        let cfg = ConfinementConfig {
            dangerously_allow_unconfined_template_resolution: true,
        };
        let template: UriTemplate = "{{ endpoint }}".try_into().unwrap();
        assert!(template.confine(&cfg, "http", "uri").is_ok());
    }

    #[test]
    fn confinement_rejects_path_traversal_and_query_injection() {
        use crate::event::Event;
        use vector_lib::event::LogEvent;
        use vrl::event_path;

        let template: UriTemplate = "https://logs.example.com/ingest/{{ tenant }}"
            .try_into()
            .unwrap();
        let template = template
            .confine(&ConfinementConfig::default(), "http", "uri")
            .unwrap();

        // Attacker tries to traverse path and inject query via tenant field
        let mut event = Event::Log(LogEvent::from("x"));
        event
            .as_mut_log()
            .insert(event_path!("tenant"), "../../evil.com/steal?data=");
        assert!(template.render_string(&event).is_err());
    }

    #[test]
    fn validate_returns_usable_values() {
        let config: HttpSinkConfig = serde_yaml::from_str(
            r#"
            uri: "http://127.0.0.1:9000/endpoint"
            encoding:
              codec: json
            "#,
        )
        .unwrap();

        let validated = config.validate().expect("validation should succeed");
        // JSON + newline-delimited framing maps to the NDJSON content type.
        assert_eq!(validated.content_type.as_deref(), Some(CONTENT_TYPE_JSON));
        assert_eq!(validated.payload_prefix, "");
        assert_eq!(validated.payload_suffix, "");
        assert!(validated.template_headers.is_empty());
        assert!(validated.converted_static_headers.is_empty());
    }
}
