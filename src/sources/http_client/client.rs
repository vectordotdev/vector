//! Generalized HTTP client source.
//! Calls an endpoint at an interval, decoding the HTTP responses into events.

use bytes::{Bytes, BytesMut};
use chrono::Utc;
use futures_util::FutureExt;
use http::{response::Parts, Uri};
use serde_with::serde_as;
use snafu::ResultExt;
use std::{collections::HashMap, time::Duration};
use tokio_util::codec::Decoder as _;
use vrl::diagnostic::Formatter;

use crate::http::{ParamType, ParameterValue, QueryParameterValue, QueryParameters};
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
use vrl::{
    compiler::{runtime::Runtime, CompileConfig, Function, Program},
    prelude::TypeState,
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

#[derive(Clone)]
pub struct Query {
    original: HashMap<String, QueryParameterValue>,
    compiled: HashMap<String, CompiledQueryParameterValue>,
    has_vrl: bool,
}

impl Query {
    pub fn new(params: &HashMap<String, QueryParameterValue>) -> Self {
        let functions = vrl::stdlib::all()
            .into_iter()
            .chain(vector_lib::enrichment::vrl_functions())
            .chain(vector_vrl_functions::all())
            .collect::<Vec<_>>();

        let compiled: HashMap<String, CompiledQueryParameterValue> = params
            .iter()
            .map(|(k, v)| (k.clone(), Self::compile_param(v, &functions)))
            .collect();

        let has_vrl = compiled.values().any(|compiled| match compiled {
            CompiledQueryParameterValue::SingleParam(param) => param.program.is_some(),
            CompiledQueryParameterValue::MultiParams(params) => {
                params.iter().any(|p| p.program.is_some())
            }
        });

        Query {
            original: params.clone(),
            compiled,
            has_vrl,
        }
    }

    fn compile_value(param: &ParameterValue, functions: &[Box<dyn Function>]) -> CompiledParam {
        let program = if param.is_vrl() {
            let state = TypeState::default();
            let config = CompileConfig::default();

            match compile_vrl(param.value(), functions, &state, config) {
                Ok(compilation_result) => {
                    if !compilation_result.warnings.is_empty() {
                        let warnings = Formatter::new(param.value(), compilation_result.warnings)
                            .colored()
                            .to_string();
                        warn!(message = "VRL compilation warnings.", %warnings, internal_log_rate_limit = true);
                    }
                    Some(compilation_result.program)
                }
                Err(diagnostics) => {
                    let error = Formatter::new(param.value(), diagnostics)
                        .colored()
                        .to_string();
                    warn!(message = "VRL compilation failed.", %error, internal_log_rate_limit = true);
                    None
                }
            }
        } else {
            None
        };

        CompiledParam {
            value: param.value().to_string(),
            program,
        }
    }

    fn compile_param(
        value: &QueryParameterValue,
        functions: &[Box<dyn Function>],
    ) -> CompiledQueryParameterValue {
        match value {
            QueryParameterValue::SingleParam(param) => CompiledQueryParameterValue::SingleParam(
                Box::new(Self::compile_value(param, functions)),
            ),
            QueryParameterValue::MultiParams(params) => {
                let compiled = params
                    .iter()
                    .map(|p| Self::compile_value(p, functions))
                    .collect();
                CompiledQueryParameterValue::MultiParams(compiled)
            }
        }
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "http_client")]
impl SourceConfig for HttpClientConfig {
    async fn build(&self, cx: SourceContext) -> crate::Result<sources::Source> {
        let query = Query::new(&self.query.clone());

        // Build the base URLs
        let endpoints = [self.endpoint.clone()];
        let urls: Vec<Uri> = endpoints
            .iter()
            .map(|s| s.parse::<Uri>().context(sources::UriParseSnafu))
            .map(|r| {
                if query.has_vrl {
                    // For URLs with VRL expressions, don't add query parameters here
                    // They'll be added dynamically during the HTTP request
                    r
                } else {
                    // For URLs without VRL expressions, add query parameters now
                    r.map(|uri| build_url(&uri, &query.original))
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
            query,
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
            warn!(message = "VRL runtime error.", source = %value, %error, internal_log_rate_limit = true);
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
        // Early exit if there is no VRL to process
        let query: &Query = &self.query;
        if !query.has_vrl {
            return None;
        }

        let mut processed_query = HashMap::new();

        for (param_name, compiled_value) in &query.compiled {
            match compiled_value {
                CompiledQueryParameterValue::SingleParam(compiled_param) => {
                    let result = match &compiled_param.program {
                        Some(prog) => resolve_vrl(&compiled_param.value, prog)?,
                        None => compiled_param.value.clone(),
                    };

                    processed_query.insert(
                        param_name.clone(),
                        QueryParameterValue::SingleParam(ParameterValue::String(result)),
                    );
                }
                CompiledQueryParameterValue::MultiParams(compiled_params) => {
                    let mut results = Vec::new();

                    for param in compiled_params {
                        let result = match &param.program {
                            Some(p) => resolve_vrl(&param.value, p)?,
                            None => param.value.clone(),
                        };
                        results.push(ParameterValue::String(result));
                    }

                    processed_query.insert(
                        param_name.clone(),
                        QueryParameterValue::MultiParams(results),
                    );
                }
            };
        }

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
