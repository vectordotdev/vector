//! Generalized HTTP client source.
//! Calls an endpoint at an interval, decoding the HTTP responses into events.

use std::{collections::HashMap, time::Duration};

use bytes::{Bytes, BytesMut};
use chrono::Utc;
use futures_util::FutureExt;
use http::{Uri, response::Parts};
use serde_with::serde_as;
use snafu::ResultExt;
use tokio_util::codec::Decoder as _;
use vector_lib::{
    TimeZone,
    codecs::{
        StreamDecodingError,
        decoding::{DeserializerConfig, FramingConfig},
    },
    compile_vrl,
    config::{LogNamespace, SourceOutput, log_schema},
    configurable::configurable_component,
    event::{Event, LogEvent, VrlTarget},
};
use vrl::{
    compiler::{CompileConfig, Function, Program, runtime::Runtime},
    prelude::TypeState,
};

use crate::{
    codecs::{Decoder, DecodingConfig},
    config::{SourceConfig, SourceContext},
    format_vrl_diagnostics,
    http::{Auth, ParamType, ParameterValue, QueryParameterValue, QueryParameters},
    serde::{default_decoding, default_framing_message_based},
    sources,
    sources::util::{
        http::HttpMethod,
        http_client,
        http_client::{
            GenericHttpClientInputs, HttpClientBuilder, build_url, call, default_interval,
            default_timeout, warn_if_interval_too_low,
        },
    },
    tls::{TlsConfig, TlsSettings},
};

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
    /// parameter values.
    #[serde(default)]
    #[configurable(metadata(
        docs::additional_props_description = "A query string parameter and its value(s)."
    ))]
    #[configurable(metadata(docs::examples = "query_examples()"))]
    pub query: QueryParameters,

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

    /// Raw data to send as the HTTP request body.
    ///
    /// Can be a static string or a VRL expression.
    ///
    /// When a body is provided, the `Content-Type` header is automatically set to
    /// `application/json` unless explicitly overridden in the `headers` configuration.
    #[serde(default)]
    pub body: Option<ParameterValue>,

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
            QueryParameterValue::SingleParam(ParameterValue::String("value".to_owned())),
        ),
        (
            "fruit".to_owned(),
            QueryParameterValue::MultiParams(vec![
                ParameterValue::String("mango".to_owned()),
                ParameterValue::String("papaya".to_owned()),
                ParameterValue::String("kiwi".to_owned()),
            ]),
        ),
        (
            "start_time".to_owned(),
            QueryParameterValue::SingleParam(ParameterValue::Typed {
                value: "now()".to_owned(),
                r#type: ParamType::Vrl,
            }),
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

/// Helper function to compile a VRL parameter value into a Program
fn compile_parameter_vrl(
    param: &ParameterValue,
    functions: &[Box<dyn Function>],
) -> Result<Option<Program>, sources::BuildError> {
    if !param.is_vrl() {
        return Ok(None);
    }

    let state = TypeState::default();
    let config = CompileConfig::default();

    match compile_vrl(param.value(), functions, &state, config) {
        Ok(compilation_result) => {
            if !compilation_result.warnings.is_empty() {
                let warnings = format_vrl_diagnostics(param.value(), compilation_result.warnings);
                warn!(message = "VRL compilation warnings.", %warnings);
            }
            Ok(Some(compilation_result.program))
        }
        Err(diagnostics) => {
            let error = format_vrl_diagnostics(param.value(), diagnostics);
            Err(sources::BuildError::VrlCompilationError {
                message: format!("VRL compilation failed: {}", error),
            })
        }
    }
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
            body: None,
            tls: None,
            auth: None,
            log_namespace: None,
        }
    }
}

impl_generate_config_from_default!(HttpClientConfig);

#[derive(Clone)]
pub struct CompiledParam {
    value: String,
    program: Option<Program>,
}

#[derive(Clone)]
pub enum CompiledQueryParameterValue {
    SingleParam(Box<CompiledParam>),
    MultiParams(Vec<CompiledParam>),
}

impl CompiledQueryParameterValue {
    fn has_vrl(&self) -> bool {
        match self {
            CompiledQueryParameterValue::SingleParam(param) => param.program.is_some(),
            CompiledQueryParameterValue::MultiParams(params) => {
                params.iter().any(|p| p.program.is_some())
            }
        }
    }
}

#[derive(Clone)]
pub struct Query {
    original: HashMap<String, QueryParameterValue>,
    compiled: HashMap<String, CompiledQueryParameterValue>,
    has_vrl: bool,
}

impl Query {
    pub fn new(params: &HashMap<String, QueryParameterValue>) -> Result<Self, sources::BuildError> {
        let functions = vector_vrl_functions::all();

        let mut compiled: HashMap<String, CompiledQueryParameterValue> = HashMap::new();

        for (k, v) in params.iter() {
            let compiled_param = Self::compile_param(v, &functions)?;
            compiled.insert(k.clone(), compiled_param);
        }

        let has_vrl = compiled.values().any(|v| v.has_vrl());

        Ok(Query {
            original: params.clone(),
            compiled,
            has_vrl,
        })
    }

    fn compile_value(
        param: &ParameterValue,
        functions: &[Box<dyn Function>],
    ) -> Result<CompiledParam, sources::BuildError> {
        let program = compile_parameter_vrl(param, functions)?;

        Ok(CompiledParam {
            value: param.value().to_string(),
            program,
        })
    }

    fn compile_param(
        value: &QueryParameterValue,
        functions: &[Box<dyn Function>],
    ) -> Result<CompiledQueryParameterValue, sources::BuildError> {
        match value {
            QueryParameterValue::SingleParam(param) => {
                Ok(CompiledQueryParameterValue::SingleParam(Box::new(
                    Self::compile_value(param, functions)?,
                )))
            }
            QueryParameterValue::MultiParams(params) => {
                let compiled = params
                    .iter()
                    .map(|p| Self::compile_value(p, functions))
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(CompiledQueryParameterValue::MultiParams(compiled))
            }
        }
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "http_client")]
impl SourceConfig for HttpClientConfig {
    async fn build(&self, cx: SourceContext) -> crate::Result<sources::Source> {
        let query = Query::new(&self.query)?;
        let functions = vector_vrl_functions::all();

        // Compile body if present
        let body = self
            .body
            .as_ref()
            .map(|body_param| -> Result<CompiledParam, sources::BuildError> {
                let program = compile_parameter_vrl(body_param, &functions)?;
                Ok(CompiledParam {
                    value: body_param.value().to_string(),
                    program,
                })
            })
            .transpose()?;

        // Build the base URLs
        let endpoints = [self.endpoint.clone()];
        let urls: Vec<Uri> = endpoints
            .iter()
            .map(|s| {
                let uri = s.parse::<Uri>().context(sources::UriParseSnafu)?;
                // For URLs with VRL expressions, add query parameters dynamically during request
                // For URLs without VRL expressions, add query parameters now
                Ok(if query.has_vrl {
                    uri
                } else {
                    build_url(&uri, &query.original)
                })
            })
            .collect::<std::result::Result<Vec<Uri>, sources::BuildError>>()?;

        let tls = TlsSettings::from_options(self.tls.as_ref())?;

        let log_namespace = cx.log_namespace(self.log_namespace);

        // build the decoder
        let decoder = self.get_decoding_config(Some(log_namespace)).build()?;

        let content_type = self.decoding.content_type(&self.framing).to_string();

        // Create context with the config for dynamic query parameter and body evaluation
        let context = HttpClientContext {
            decoder,
            log_namespace,
            query,
            body,
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
    query: Query,
    body: Option<CompiledParam>,
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

fn resolve_vrl(value: &str, program: &Program) -> Option<String> {
    let mut target = VrlTarget::new(Event::Log(LogEvent::default()), program.info(), false);
    let timezone = TimeZone::default();

    Runtime::default()
        .resolve(&mut target, program, &timezone)
        .map_err(|error| {
            warn!(message = "VRL runtime error.", source = %value, %error);
        })
        .ok()
        .and_then(|vrl_value| {
            let json_value = serde_json::to_value(vrl_value).ok()?;

            // Properly handle VRL values, so that key1: `upcase("foo")` will resolve
            // properly as endpoint.com/key1=FOO and not endpoint.com/key1="FOO"
            // similarly, `now()` should resolve to endpoint.com/key1=2025-06-07T10:39:08.662735Z
            // and not endpoint.com/key1=t'2025-06-07T10:39:08.662735Z'
            let resolved_string = match json_value {
                serde_json::Value::String(s) => s,
                value => value.to_string(),
            };
            Some(resolved_string)
        })
}

/// Resolve a compiled parameter, handling VRL evaluation if present
fn resolve_compiled_param(compiled: &CompiledParam) -> Option<String> {
    match &compiled.program {
        Some(program) => resolve_vrl(&compiled.value, program),
        None => Some(compiled.value.clone()),
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

    /// Get the request body to send with the HTTP request
    fn get_request_body(&self) -> Option<String> {
        self.body.as_ref().and_then(resolve_compiled_param)
    }

    /// Process the URL dynamically before each request
    fn process_url(&self, url: &Uri) -> Option<Uri> {
        if !self.query.has_vrl {
            return None;
        }

        // Resolve all query parameters with VRL expressions
        let processed_query: Option<HashMap<_, _>> = self
            .query
            .compiled
            .iter()
            .map(|(name, value)| {
                let resolved = match value {
                    CompiledQueryParameterValue::SingleParam(param) => {
                        let result = resolve_compiled_param(param)?;
                        QueryParameterValue::SingleParam(ParameterValue::String(result))
                    }
                    CompiledQueryParameterValue::MultiParams(params) => {
                        let results: Option<Vec<_>> = params
                            .iter()
                            .map(|p| resolve_compiled_param(p).map(ParameterValue::String))
                            .collect();
                        QueryParameterValue::MultiParams(results?)
                    }
                };
                Some((name.clone(), resolved))
            })
            .collect();

        // Build base URI and add query parameters
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

        Some(build_url(&base_uri, &processed_query?))
    }

    /// Enriches events with source_type, timestamp
    fn enrich_events(&mut self, events: &mut Vec<Event>) {
        let now = Utc::now();

        for event in events {
            match event {
                Event::Log(log) => {
                    self.log_namespace.insert_standard_vector_source_metadata(
                        log,
                        HttpClientConfig::NAME,
                        now,
                    );
                }
                Event::Metric(metric) => {
                    if let Some(source_type_key) = log_schema().source_type_key() {
                        metric.replace_tag(
                            source_type_key.to_string(),
                            HttpClientConfig::NAME.to_string(),
                        );
                    }
                }
                Event::Trace(trace) => {
                    trace.maybe_insert(log_schema().source_type_key_target_path(), || {
                        Bytes::from(HttpClientConfig::NAME).into()
                    });
                }
            }
        }
    }
}
