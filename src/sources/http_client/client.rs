//! Generalized HTTP client source.
//! Calls an endpoint at an interval, decoding the HTTP responses into events.

use bytes::{Bytes, BytesMut};
use chrono::Utc;
use futures_util::FutureExt;
use http::{response::Parts, Uri};
use regex::Regex;
use serde_with::serde_as;
use snafu::ResultExt;
use std::sync::LazyLock;
use std::{collections::HashMap, time::Duration};
use tokio_util::codec::Decoder as _;
use vrl::diagnostic::Formatter;

use crate::http::{QueryParameterValue, QueryParameters};
use crate::sources::util::http_client;
use crate::{
    codecs::{Decoder, DecodingConfig},
    config::{SourceConfig, SourceContext},
    http::Auth,
    serde::{default_decoding, default_framing_message_based},
    sources,
    sources::util::{
        http::HttpMethod,
        http_client::{
            build_url, call, default_interval, default_timeout, warn_if_interval_too_low,
            GenericHttpClientInputs, HttpClientBuilder,
        },
    },
    tls::{TlsConfig, TlsSettings},
    Result,
};
use vector_lib::codecs::{
    decoding::{DeserializerConfig, FramingConfig},
    StreamDecodingError,
};
use vector_lib::config::{log_schema, LogNamespace, SourceOutput};
use vector_lib::configurable::configurable_component;
use vector_lib::{
    compile_vrl,
    event::{Event, LogEvent, VrlTarget},
    TimeZone,
};
use vrl::compiler::CompilationResult;
use vrl::{
    compiler::{runtime::Runtime, CompileConfig, Function},
    core::Value,
    prelude::TypeState,
};

static RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\{\{(?P<key>.+)\}\}").unwrap());
static FUNCTIONS: LazyLock<Vec<Box<dyn Function>>> = LazyLock::new(|| {
    vrl::stdlib::all()
        .into_iter()
        .chain(vector_lib::enrichment::vrl_functions())
        .chain(vector_vrl_functions::all())
        .collect()
});

/// Configuration for the `http_client` source.
#[serde_as]
#[configurable_component(source(
    "http_client",
    "Pull observability data from an HTTP server at a configured interval."
))]
#[derive(Clone, Debug)]
pub struct HttpClientConfig {
    /// The HTTP endpoint to collect events from.
    ///
    /// The full path must be specified.
    #[configurable(metadata(docs::examples = "http://127.0.0.1:9898/logs"))]
    pub endpoint: String,

    /// The interval between scrapes. Requests are run concurrently so if a scrape takes longer
    /// than the interval a new scrape will be started. This can take extra resources, set the timeout
    /// to a value lower than the scrape interval to prevent this from happening.
    #[serde(default = "default_interval")]
    #[serde_as(as = "serde_with::DurationSeconds<u64>")]
    #[serde(rename = "scrape_interval_secs")]
    #[configurable(metadata(docs::human_name = "Scrape Interval"))]
    pub interval: Duration,

    /// The timeout for each scrape request.
    #[serde(default = "default_timeout")]
    #[serde_as(as = "serde_with:: DurationSecondsWithFrac<f64>")]
    #[serde(rename = "scrape_timeout_secs")]
    #[configurable(metadata(docs::human_name = "Scrape Timeout"))]
    pub timeout: Duration,

    /// Custom parameters for the HTTP request query string.
    ///
    /// One or more values for the same parameter key can be provided.
    ///
    /// The parameters provided in this option are appended to any parameters
    /// manually provided in the `endpoint` option.
    ///
    /// VRL functions are supported within query parameters. You can
    /// use functions like `now()` to dynamically modify query
    /// parameter values. The VRL function must be wrapped in `{{ ... }}`
    /// e.g. {{ now() }}
    #[serde(default)]
    #[configurable(metadata(
        docs::additional_props_description = "A query string parameter and its value(s)."
    ))]
    #[configurable(metadata(docs::examples = "query_examples()"))]
    pub query: QueryParameters,

    /// Decoder to use on the HTTP responses.
    #[configurable(derived)]
    #[serde(default = "default_decoding")]
    pub decoding: DeserializerConfig,

    /// Framing to use in the decoding.
    #[configurable(derived)]
    #[serde(default = "default_framing_message_based")]
    pub framing: FramingConfig,

    /// Headers to apply to the HTTP requests.
    ///
    /// One or more values for the same header can be provided.
    #[serde(default)]
    #[configurable(metadata(
        docs::additional_props_description = "An HTTP request header and its value(s)."
    ))]
    #[configurable(metadata(docs::examples = "headers_examples()"))]
    pub headers: HashMap<String, Vec<String>>,

    /// Specifies the method of the HTTP request.
    #[serde(default = "default_http_method")]
    pub method: HttpMethod,

    /// TLS configuration.
    #[configurable(derived)]
    pub tls: Option<TlsConfig>,

    /// HTTP Authentication.
    #[configurable(derived)]
    pub auth: Option<Auth>,

    /// The namespace to use for logs. This overrides the global setting.
    #[configurable(metadata(docs::hidden))]
    #[serde(default)]
    pub log_namespace: Option<bool>,
}

const fn default_http_method() -> HttpMethod {
    HttpMethod::Get
}

fn query_examples() -> QueryParameters {
    HashMap::<_, _>::from_iter([
        (
            "field".to_owned(),
            QueryParameterValue::SingleParam("value".to_owned()),
        ),
        (
            "fruit".to_owned(),
            QueryParameterValue::MultiParams(vec![
                "mango".to_owned(),
                "papaya".to_owned(),
                "kiwi".to_owned(),
            ]),
        ),
        (
            "start_time".to_owned(),
            QueryParameterValue::SingleParam("\"{{ now() }}\"".to_owned()),
        ),
    ])
}

fn headers_examples() -> HashMap<String, Vec<String>> {
    HashMap::<_, _>::from_iter([
        (
            "Accept".to_owned(),
            vec!["text/plain".to_owned(), "text/html".to_owned()],
        ),
        (
            "X-My-Custom-Header".to_owned(),
            vec![
                "a".to_owned(),
                "vector".to_owned(),
                "of".to_owned(),
                "values".to_owned(),
            ],
        ),
    ])
}

impl Default for HttpClientConfig {
    fn default() -> Self {
        Self {
            endpoint: "http://localhost:9898/logs".to_string(),
            query: HashMap::new(),
            interval: default_interval(),
            timeout: default_timeout(),
            decoding: default_decoding(),
            framing: default_framing_message_based(),
            headers: HashMap::new(),
            method: default_http_method(),
            tls: None,
            auth: None,
            log_namespace: None,
        }
    }
}

impl_generate_config_from_default!(HttpClientConfig);

fn process_vrl(query_str: &str) -> Option<String> {
    let state = TypeState::default();
    let mut config = CompileConfig::default();
    config.set_read_only();

    // Strip {{ }} from the string
    let raw_vrl_string = RE.replace_all(query_str, |caps: &regex::Captures| {
        let expression = &caps[1];
        expression.to_string()
    });

    match compile_vrl(&raw_vrl_string, &FUNCTIONS, &state, config) {
        Ok(CompilationResult {
            program,
            warnings,
            config: _,
        }) => {
            if !warnings.is_empty() {
                let warnings_str = Formatter::new(&raw_vrl_string, warnings)
                    .colored()
                    .to_string();
                warn!(message = "VRL compilation warning.", %warnings_str);
            }

            let mut target = VrlTarget::new(
                Event::Log(LogEvent::from(Value::from(raw_vrl_string))),
                program.info(),
                false,
            );

            let timezone = TimeZone::default();
            if let Ok(value) = Runtime::default().resolve(&mut target, &program, &timezone) {
                // Trim quotes from the string, so that key1: {{ upcase("foo")}} will resolve
                // properly as endpoint.com/key1=FOO and not endpoint.com/key1="FOO"
                return Some(value.to_string().trim_matches('"').to_string());
            }
        }
        Err(diagnostics) => {
            let error_str = Formatter::new(&raw_vrl_string, diagnostics)
                .colored()
                .to_string();
            warn!(message = "VRL compilation error.", %error_str);
        }
    }
    None
}

#[async_trait::async_trait]
#[typetag::serde(name = "http_client")]
impl SourceConfig for HttpClientConfig {
    async fn build(&self, cx: SourceContext) -> Result<sources::Source> {
        // Check if we have any VRL expressions in the query parameters
        let has_vrl_expressions = self.query.iter().any(|(_, value)| match value {
            QueryParameterValue::SingleParam(param) => RE.is_match(param),
            QueryParameterValue::MultiParams(params) => params.iter().any(|p| RE.is_match(p)),
        });

        // Build the base URLs
        let endpoints = [self.endpoint.clone()];
        let urls: Vec<Uri> = endpoints
            .iter()
            .map(|s| s.parse::<Uri>().context(sources::UriParseSnafu))
            .map(|r| {
                if has_vrl_expressions {
                    // For URLs with VRL expressions, don't add query parameters here
                    // They'll be added dynamically during the HTTP request
                    r
                } else {
                    // For URLs without VRL expressions, add query parameters now
                    r.map(|uri| build_url(&uri, &self.query))
                }
            })
            .collect::<std::result::Result<Vec<Uri>, sources::BuildError>>()?;

        let tls = TlsSettings::from_options(self.tls.as_ref())?;

        let log_namespace = cx.log_namespace(self.log_namespace);

        // build the decoder
        let decoder = self.get_decoding_config(Some(log_namespace)).build()?;

        let content_type = self.decoding.content_type(&self.framing).to_string();

        // Create context with the config for dynamic query parameter evaluation
        let context = HttpClientContext {
            decoder,
            log_namespace,
            config: if has_vrl_expressions {
                Some(self.clone())
            } else {
                None
            },
        };

        warn_if_interval_too_low(self.timeout, self.interval);

        let inputs = GenericHttpClientInputs {
            urls,
            interval: self.interval,
            timeout: self.timeout,
            headers: self.headers.clone(),
            content_type,
            auth: self.auth.clone(),
            tls,
            proxy: cx.proxy.clone(),
            shutdown: cx.shutdown,
        };

        Ok(call(inputs, context, cx.out, self.method).boxed())
    }

    fn outputs(&self, global_log_namespace: LogNamespace) -> Vec<SourceOutput> {
        // There is a global and per-source `log_namespace` config. The source config overrides the global setting,
        // and is merged here.
        let log_namespace = global_log_namespace.merge(self.log_namespace);

        let schema_definition = self
            .decoding
            .schema_definition(log_namespace)
            .with_standard_vector_source_metadata();

        vec![SourceOutput::new_maybe_logs(
            self.decoding.output_type(),
            schema_definition,
        )]
    }

    fn can_acknowledge(&self) -> bool {
        false
    }
}

impl HttpClientConfig {
    pub fn get_decoding_config(&self, log_namespace: Option<LogNamespace>) -> DecodingConfig {
        let decoding = self.decoding.clone();
        let framing = self.framing.clone();
        let log_namespace =
            log_namespace.unwrap_or_else(|| self.log_namespace.unwrap_or(false).into());

        DecodingConfig::new(framing, decoding, log_namespace)
    }
}

/// Captures the configuration options required to decode the incoming requests into events.
#[derive(Clone)]
pub struct HttpClientContext {
    pub decoder: Decoder,
    pub log_namespace: LogNamespace,
    /// The original config is stored for dynamic query parameter evaluation
    /// This is used to evaluate VRL expressions like {{ now() }} on each request
    /// rather than just once during initialization
    config: Option<HttpClientConfig>,
}

impl HttpClientContext {
    /// Decode the events from the byte buffer
    fn decode_events(&mut self, buf: &mut BytesMut) -> Vec<Event> {
        let mut events = Vec::new();
        loop {
            match self.decoder.decode_eof(buf) {
                Ok(Some((next, _))) => {
                    events.extend(next);
                }
                Ok(None) => break,
                Err(error) => {
                    // Error is logged by `crate::codecs::Decoder`, no further
                    // handling is needed here.
                    if !error.can_continue() {
                        break;
                    }
                    break;
                }
            }
        }
        events
    }
}

impl HttpClientBuilder for HttpClientContext {
    type Context = HttpClientContext;

    /// No additional context from request data is needed from this particular client.
    fn build(&self, _uri: &Uri) -> Self::Context {
        self.clone()
    }
}

/// Process any VRL expressions in query parameters
fn process_query_value(value: &QueryParameterValue) -> QueryParameterValue {
    match value {
        QueryParameterValue::SingleParam(param) => QueryParameterValue::SingleParam(
            process_vrl(param)
                .filter(|_| RE.is_match(param))
                .unwrap_or_else(|| param.to_string()),
        ),
        QueryParameterValue::MultiParams(params) => QueryParameterValue::MultiParams(
            params
                .iter()
                .map(|param| {
                    process_vrl(param)
                        .filter(|_| RE.is_match(param))
                        .unwrap_or_else(|| param.to_string())
                })
                .collect(),
        ),
    }
}

impl http_client::HttpClientContext for HttpClientContext {
    /// Decodes the HTTP response body into events per the decoder configured.
    fn on_response(&mut self, _url: &Uri, _header: &Parts, body: &Bytes) -> Option<Vec<Event>> {
        // get the body into a byte array
        let mut buf = BytesMut::new();
        buf.extend_from_slice(body);

        let events = self.decode_events(&mut buf);

        Some(events)
    }

    /// Process the URL dynamically before each request
    fn process_url(&self, url: &Uri) -> Option<Uri> {
        // If we don't have a config, it means the query doesn't have any VRL parameters,
        // so we can short-circuit here and just use the original URL
        let config: &HttpClientConfig = self.config.as_ref()?;

        let mut processed_query = HashMap::new();
        let mut has_vrl_params = false;

        // Process query parameters for each request
        for (param_name, query_value) in &config.query {
            let processed_value = process_query_value(query_value);

            // Check if any parameters were modified
            if processed_value != *query_value {
                has_vrl_params = true;
            }

            // Store the processed query parameter
            processed_query.insert(param_name.to_string(), processed_value);
        }

        if has_vrl_params {
            // Extract the base URI without query parameters to avoid parameter duplication
            let base_uri = Uri::builder()
                .scheme(
                    url.scheme()
                        .cloned()
                        .unwrap_or_else(|| http::uri::Scheme::try_from("http").unwrap()),
                )
                .authority(
                    url.authority()
                        .cloned()
                        .unwrap_or_else(|| http::uri::Authority::try_from("localhost").unwrap()),
                )
                .path_and_query(url.path().to_string())
                .build()
                .ok()?;

            Some(build_url(&base_uri, &processed_query))
        } else {
            None
        }
    }

    /// Enriches events with source_type, timestamp
    fn enrich_events(&mut self, events: &mut Vec<Event>) {
        let now = Utc::now();

        for event in events {
            match event {
                Event::Log(ref mut log) => {
                    self.log_namespace.insert_standard_vector_source_metadata(
                        log,
                        HttpClientConfig::NAME,
                        now,
                    );
                }
                Event::Metric(ref mut metric) => {
                    if let Some(source_type_key) = log_schema().source_type_key() {
                        metric.replace_tag(
                            source_type_key.to_string(),
                            HttpClientConfig::NAME.to_string(),
                        );
                    }
                }
                Event::Trace(ref mut trace) => {
                    trace.maybe_insert(log_schema().source_type_key_target_path(), || {
                        Bytes::from(HttpClientConfig::NAME).into()
                    });
                }
            }
        }
    }
}
