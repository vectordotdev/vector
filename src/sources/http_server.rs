use std::{collections::HashMap, net::SocketAddr};

use bytes::{Bytes, BytesMut};
use chrono::Utc;
use http::StatusCode;
use http_serde;
use tokio_util::codec::Decoder as _;
use vector_lib::{
    codecs::decoding::{DeserializerConfig, FramingConfig},
    config::{DataType, LegacyKey, LogNamespace},
    configurable::configurable_component,
    lookup::{lookup_v2::OptionalValuePath, owned_value_path, path},
    schema::Definition,
};
use vrl::value::{Kind, kind::Collection};
use warp::http::HeaderMap;

use crate::{
    codecs::{Decoder, DecodingConfig},
    common::http::{ErrorMessage, server_auth::HttpServerAuthConfig},
    config::{
        GenerateConfig, Resource, SourceAcknowledgementsConfig, SourceConfig, SourceContext,
        SourceOutput,
    },
    event::Event,
    http::KeepaliveConfig,
    serde::{bool_or_struct, default_decoding},
    sources::util::{
        HttpSource,
        http::{HttpMethod, add_headers, add_query_parameters},
    },
    tls::TlsEnableableConfig,
};

/// Configuration for the `http` source.
#[configurable_component(source("http", "Host an HTTP endpoint to receive logs."))]
#[configurable(metadata(deprecated))]
#[derive(Clone, Debug)]
pub struct HttpConfig(SimpleHttpConfig);

impl GenerateConfig for HttpConfig {
    fn generate_config() -> serde_json::Value {
        <SimpleHttpConfig as GenerateConfig>::generate_config()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "http")]
impl SourceConfig for HttpConfig {
    async fn build(&self, cx: SourceContext) -> vector_lib::Result<super::Source> {
        self.0.build(cx).await
    }

    fn outputs(&self, global_log_namespace: LogNamespace) -> Vec<SourceOutput> {
        self.0.outputs(global_log_namespace)
    }

    fn resources(&self) -> Vec<Resource> {
        self.0.resources()
    }

    fn can_acknowledge(&self) -> bool {
        self.0.can_acknowledge()
    }
}

/// Configuration for the `http_server` source.
#[configurable_component(source("http_server", "Host an HTTP endpoint to receive logs."))]
#[derive(Clone, Debug)]
pub struct SimpleHttpConfig {
    /// The socket address to listen for connections on.
    ///
    /// It _must_ include a port.
    #[configurable(metadata(docs::examples = "0.0.0.0:80"))]
    #[configurable(metadata(docs::examples = "localhost:80"))]
    address: SocketAddr,

    /// A list of HTTP headers to include in the log event.
    ///
    /// Accepts the wildcard (`*`) character for headers matching a specified pattern.
    ///
    /// Specifying "*" results in all headers included in the log event.
    ///
    /// These headers are not included in the JSON payload if a field with a conflicting name exists.
    #[serde(default)]
    #[configurable(metadata(docs::examples = "User-Agent"))]
    #[configurable(metadata(docs::examples = "X-My-Custom-Header"))]
    #[configurable(metadata(docs::examples = "X-*"))]
    #[configurable(metadata(docs::examples = "*"))]
    headers: Vec<String>,

    /// A list of URL query parameters to include in the log event.
    ///
    /// Accepts the wildcard (`*`) character for query parameters matching a specified pattern.
    ///
    /// Specifying "*" results in all query parameters included in the log event.
    ///
    /// These override any values included in the body with conflicting names.
    #[serde(default)]
    #[configurable(metadata(docs::examples = "application"))]
    #[configurable(metadata(docs::examples = "source"))]
    #[configurable(metadata(docs::examples = "param*"))]
    #[configurable(metadata(docs::examples = "*"))]
    query_parameters: Vec<String>,

    /// HTTP authentication configuration.
    ///
    /// Use HTTP authentication with HTTPS only. The authentication credentials are passed as an
    /// HTTP header without any additional encryption beyond what is provided by the transport itself.
    ///
    /// When using the `custom` strategy, the VRL program may write `%field = value` to enrich
    /// authenticated events. These metadata fields are injected into the event body (legacy
    /// namespace) or under `http_server.<field>` in event metadata (Vector namespace).
    #[configurable(derived)]
    auth: Option<HttpServerAuthConfig>,

    /// Whether or not to treat the configured `path` as an absolute path.
    ///
    /// If set to `true`, only requests using the exact URL path specified in `path` are accepted. Otherwise,
    /// requests sent to a URL path that starts with the value of `path` are accepted.
    ///
    /// With `strict_path` set to `false` and `path` set to `""`, the configured HTTP source accepts requests from
    /// any URL path.
    #[serde(default = "crate::serde::default_true")]
    strict_path: bool,

    /// The URL path on which log event POST requests are sent.
    #[serde(default = "default_path")]
    #[configurable(metadata(docs::examples = "/event/path"))]
    #[configurable(metadata(docs::examples = "/logs"))]
    path: String,

    /// The event key in which the requested URL path used to send the request is stored.
    #[serde(default = "default_path_key")]
    #[configurable(metadata(docs::examples = "vector_http_path"))]
    path_key: OptionalValuePath,

    /// If set, the name of the log field used to add the remote IP to each event
    #[serde(default = "default_host_key")]
    #[configurable(metadata(docs::examples = "hostname"))]
    host_key: OptionalValuePath,

    /// Specifies the action of the HTTP request.
    #[serde(default = "default_http_method")]
    method: HttpMethod,

    /// Specifies the HTTP response status code that will be returned on successful requests.
    #[configurable(metadata(docs::examples = 202))]
    #[configurable(metadata(docs::numeric_type = "uint"))]
    #[serde(with = "http_serde::status_code")]
    #[serde(default = "default_http_response_code")]
    response_code: StatusCode,

    #[configurable(derived)]
    tls: Option<TlsEnableableConfig>,

    #[configurable(derived)]
    framing: Option<FramingConfig>,

    #[configurable(derived)]
    decoding: Option<DeserializerConfig>,

    #[configurable(derived)]
    #[serde(default, deserialize_with = "bool_or_struct")]
    acknowledgements: SourceAcknowledgementsConfig,

    /// The namespace to use for logs. This overrides the global setting.
    #[configurable(metadata(docs::hidden))]
    #[serde(default)]
    log_namespace: Option<bool>,

    #[configurable(derived)]
    #[serde(default)]
    keepalive: KeepaliveConfig,
}

impl SimpleHttpConfig {
    /// Builds the `schema::Definition` for this source using the provided `LogNamespace`.
    fn schema_definition(&self, log_namespace: LogNamespace) -> Definition {
        let mut schema_definition = self
            .decoding
            .as_ref()
            .unwrap_or(&default_decoding())
            .schema_definition(log_namespace)
            .with_source_metadata(
                SimpleHttpConfig::NAME,
                self.path_key.path.clone().map(LegacyKey::InsertIfEmpty),
                &owned_value_path!("path"),
                Kind::bytes(),
                None,
            )
            // for metadata that is added to the events dynamically from the self.headers
            .with_source_metadata(
                SimpleHttpConfig::NAME,
                None,
                &owned_value_path!("headers"),
                Kind::object(Collection::empty().with_unknown(Kind::bytes())).or_undefined(),
                None,
            )
            // for metadata that is added to the events dynamically from the self.query_parameters
            .with_source_metadata(
                SimpleHttpConfig::NAME,
                None,
                &owned_value_path!("query_parameters"),
                Kind::object(Collection::empty().with_unknown(Kind::bytes())).or_undefined(),
                None,
            )
            .with_source_metadata(
                SimpleHttpConfig::NAME,
                self.host_key.path.clone().map(LegacyKey::Overwrite),
                &owned_value_path!("host"),
                Kind::bytes().or_undefined(),
                None,
            )
            .with_standard_vector_source_metadata();

        // For metadata that is added to the events dynamically from config options.
        if log_namespace == LogNamespace::Legacy {
            // Custom auth programs can inject any VRL value, not just bytes; widen the unknown
            // field kind accordingly so schema-aware downstream components don't reject events.
            let unknown_kind = if matches!(self.auth, Some(HttpServerAuthConfig::Custom { .. })) {
                Kind::any()
            } else {
                Kind::bytes()
            };
            schema_definition = schema_definition.unknown_fields(unknown_kind);
        }

        schema_definition
    }

    fn get_decoding_config(&self) -> crate::Result<DecodingConfig> {
        let decoding = self.decoding.clone().unwrap_or_else(default_decoding);
        let framing = self
            .framing
            .clone()
            .unwrap_or_else(|| decoding.default_stream_framing());

        Ok(DecodingConfig::new(
            framing,
            decoding,
            self.log_namespace.unwrap_or(false).into(),
        ))
    }
}

impl Default for SimpleHttpConfig {
    fn default() -> Self {
        Self {
            address: "0.0.0.0:8080".parse().unwrap(),
            headers: Vec::new(),
            query_parameters: Vec::new(),
            tls: None,
            auth: None,
            path: default_path(),
            path_key: default_path_key(),
            host_key: default_host_key(),
            method: default_http_method(),
            response_code: default_http_response_code(),
            strict_path: true,
            framing: None,
            decoding: Some(default_decoding()),
            acknowledgements: SourceAcknowledgementsConfig::default(),
            log_namespace: None,
            keepalive: KeepaliveConfig::default(),
        }
    }
}

impl_generate_config_from_default!(SimpleHttpConfig);

const fn default_http_method() -> HttpMethod {
    HttpMethod::Post
}

fn default_path() -> String {
    "/".to_string()
}

fn default_path_key() -> OptionalValuePath {
    OptionalValuePath::from(owned_value_path!("path"))
}

fn default_host_key() -> OptionalValuePath {
    OptionalValuePath::none()
}

const fn default_http_response_code() -> StatusCode {
    StatusCode::OK
}

/// Removes duplicates from the list, and logs a `warn!()` for each duplicate removed.
pub fn remove_duplicates(mut list: Vec<String>, list_name: &str) -> Vec<String> {
    list.sort();

    let mut dedup = false;
    for (idx, name) in list.iter().enumerate() {
        if idx < list.len() - 1 && list[idx] == list[idx + 1] {
            warn!(
                "`{}` configuration contains duplicate entry for `{}`. Removing duplicate.",
                list_name, name
            );
            dedup = true;
        }
    }

    if dedup {
        list.dedup();
    }
    list
}

/// Convert [`SocketAddr`] into a string, returning only the IP address.
fn socket_addr_to_ip_string(addr: &SocketAddr) -> String {
    addr.ip().to_string()
}

#[derive(Clone)]
pub enum HttpConfigParamKind {
    Glob(glob::Pattern),
    Exact(String),
}

pub fn build_param_matcher(list: &[String]) -> crate::Result<Vec<HttpConfigParamKind>> {
    list.iter()
        .map(|s| match s.contains('*') {
            true => Ok(HttpConfigParamKind::Glob(glob::Pattern::new(s)?)),
            false => Ok(HttpConfigParamKind::Exact(s.to_string())),
        })
        .collect::<crate::Result<Vec<HttpConfigParamKind>>>()
}

#[async_trait::async_trait]
#[typetag::serde(name = "http_server")]
impl SourceConfig for SimpleHttpConfig {
    async fn build(&self, cx: SourceContext) -> crate::Result<super::Source> {
        let log_namespace = cx.log_namespace(self.log_namespace);
        let decoder = self
            .get_decoding_config()?
            .build()?
            .with_log_namespace(log_namespace);

        let source = SimpleHttpSource {
            headers: build_param_matcher(&remove_duplicates(self.headers.clone(), "headers"))?,
            query_parameters: build_param_matcher(&remove_duplicates(
                self.query_parameters.clone(),
                "query_parameters",
            ))?,
            path_key: self.path_key.clone(),
            host_key: self.host_key.clone(),
            decoder,
            log_namespace,
        };
        source.run(
            self.address,
            self.path.as_str(),
            self.method,
            self.response_code,
            self.strict_path,
            self.tls.as_ref(),
            self.auth.as_ref(),
            cx,
            self.acknowledgements,
            self.keepalive.clone(),
        )
    }

    fn outputs(&self, global_log_namespace: LogNamespace) -> Vec<SourceOutput> {
        // There is a global and per-source `log_namespace` config.
        // The source config overrides the global setting and is merged here.
        let log_namespace = global_log_namespace.merge(self.log_namespace);

        let schema_definition = self.schema_definition(log_namespace);

        vec![SourceOutput::new_maybe_logs(
            self.decoding
                .as_ref()
                .map(|d| d.output_type())
                .unwrap_or(DataType::Log),
            schema_definition,
        )]
    }

    fn resources(&self) -> Vec<Resource> {
        vec![Resource::tcp(self.address)]
    }

    fn can_acknowledge(&self) -> bool {
        true
    }
}

#[derive(Clone)]
struct SimpleHttpSource {
    headers: Vec<HttpConfigParamKind>,
    query_parameters: Vec<HttpConfigParamKind>,
    path_key: OptionalValuePath,
    host_key: OptionalValuePath,
    decoder: Decoder,
    log_namespace: LogNamespace,
}

impl HttpSource for SimpleHttpSource {
    fn log_namespace(&self) -> LogNamespace {
        self.log_namespace
    }

    fn name() -> &'static str {
        SimpleHttpConfig::NAME
    }

    /// Enriches the log events with metadata for the `request_path` and for each of the headers.
    /// Non-log events are skipped.
    fn enrich_events(
        &self,
        events: &mut [Event],
        request_path: &str,
        headers: &HeaderMap,
        query_parameters: &HashMap<String, String>,
        source_ip: Option<&SocketAddr>,
    ) {
        let now = Utc::now();
        for event in events.iter_mut() {
            match event {
                Event::Log(log) => {
                    // add request_path to each event
                    self.log_namespace.insert_source_metadata(
                        SimpleHttpConfig::NAME,
                        log,
                        self.path_key.path.as_ref().map(LegacyKey::InsertIfEmpty),
                        path!("path"),
                        request_path.to_owned(),
                    );

                    self.log_namespace.insert_standard_vector_source_metadata(
                        log,
                        SimpleHttpConfig::NAME,
                        now,
                    );

                    if let Some(addr) = source_ip {
                        self.log_namespace.insert_source_metadata(
                            SimpleHttpConfig::NAME,
                            log,
                            self.host_key.path.as_ref().map(LegacyKey::Overwrite),
                            path!("host"),
                            socket_addr_to_ip_string(addr),
                        );
                    }
                }
                _ => {
                    continue;
                }
            }
        }

        add_headers(
            events,
            &self.headers,
            headers,
            self.log_namespace,
            SimpleHttpConfig::NAME,
        );

        add_query_parameters(
            events,
            &self.query_parameters,
            query_parameters,
            self.log_namespace,
            SimpleHttpConfig::NAME,
        );
    }

    fn build_events(
        &self,
        body: Bytes,
        _header_map: &HeaderMap,
        _query_parameters: &HashMap<String, String>,
        _request_path: &str,
    ) -> Result<Vec<Event>, ErrorMessage> {
        let mut decoder = self.decoder.clone();
        let mut events = Vec::new();
        let mut bytes = BytesMut::new();
        bytes.extend_from_slice(&body);

        loop {
            match decoder.decode_eof(&mut bytes) {
                Ok(Some((next, _))) => {
                    events.extend(next);
                }
                Ok(None) => break,
                Err(error) => {
                    // Error is logged / emitted by `vector_lib::codecs::Decoder`, no further
                    // handling is needed here
                    return Err(ErrorMessage::new(
                        StatusCode::BAD_REQUEST,
                        format!("Failed decoding body: {error}"),
                    ));
                }
            }
        }

        Ok(events)
    }

    fn enable_source_ip(&self) -> bool {
        self.host_key.path.is_some()
    }
}

#[cfg(test)]
mod tests;
