use crate::common::http::{server_auth::HttpServerAuthConfig, ErrorMessage};
use std::{collections::HashMap, net::SocketAddr};

use bytes::{Bytes, BytesMut};
use chrono::Utc;
use http::StatusCode;
use http_serde;
use tokio_util::codec::Decoder as _;
use vrl::value::{kind::Collection, Kind};
use warp::http::HeaderMap;

use vector_lib::codecs::{
    decoding::{DeserializerConfig, FramingConfig},
    BytesDecoderConfig, BytesDeserializerConfig, JsonDeserializerConfig,
    NewlineDelimitedDecoderConfig,
};
use vector_lib::configurable::configurable_component;
use vector_lib::lookup::{lookup_v2::OptionalValuePath, owned_value_path, path};
use vector_lib::{
    config::{DataType, LegacyKey, LogNamespace},
    schema::Definition,
};

use crate::{
    codecs::{Decoder, DecodingConfig},
    config::{
        GenerateConfig, Resource, SourceAcknowledgementsConfig, SourceConfig, SourceContext,
        SourceOutput,
    },
    event::Event,
    http::KeepaliveConfig,
    serde::{bool_or_struct, default_decoding},
    sources::util::{
        http::{add_headers, add_query_parameters, HttpMethod},
        Encoding, HttpSource,
    },
    tls::TlsEnableableConfig,
};

/// Configuration for the `http` source.
#[configurable_component(source("http", "Host an HTTP endpoint to receive logs."))]
#[configurable(metadata(deprecated))]
#[derive(Clone, Debug)]
pub struct HttpConfig(SimpleHttpConfig);

impl GenerateConfig for HttpConfig {
    fn generate_config() -> toml::Value {
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

    /// The expected encoding of received data.
    ///
    /// For `json` and `ndjson` encodings, the fields of the JSON objects are output as separate fields.
    #[serde(default)]
    encoding: Option<Encoding>,

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

        // for metadata that is added to the events dynamically from config options
        if log_namespace == LogNamespace::Legacy {
            schema_definition = schema_definition.unknown_fields(Kind::bytes());
        }

        schema_definition
    }

    fn get_decoding_config(&self) -> crate::Result<DecodingConfig> {
        if self.encoding.is_some() && (self.framing.is_some() || self.decoding.is_some()) {
            return Err("Using `encoding` is deprecated and does not have any effect when `decoding` or `framing` is provided. Configure `framing` and `decoding` instead.".into());
        }

        let (framing, decoding) = if let Some(encoding) = self.encoding {
            match encoding {
                Encoding::Text => (
                    NewlineDelimitedDecoderConfig::new().into(),
                    BytesDeserializerConfig::new().into(),
                ),
                Encoding::Json => (
                    BytesDecoderConfig::new().into(),
                    JsonDeserializerConfig::default().into(),
                ),
                Encoding::Ndjson => (
                    NewlineDelimitedDecoderConfig::new().into(),
                    JsonDeserializerConfig::default().into(),
                ),
                Encoding::Binary => (
                    BytesDecoderConfig::new().into(),
                    BytesDeserializerConfig::new().into(),
                ),
            }
        } else {
            let decoding = self.decoding.clone().unwrap_or_else(default_decoding);
            let framing = self
                .framing
                .clone()
                .unwrap_or_else(|| decoding.default_stream_framing());
            (framing, decoding)
        };

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
            encoding: None,
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
                    // Error is logged / emitted by `crate::codecs::Decoder`, no further
                    // handling is needed here
                    return Err(ErrorMessage::new(
                        StatusCode::BAD_REQUEST,
                        format!("Failed decoding body: {}", error),
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
mod tests {
    use std::str::FromStr;
    use std::{io::Write, net::SocketAddr};

    use flate2::{
        write::{GzEncoder, ZlibEncoder},
        Compression,
    };
    use futures::Stream;
    use headers::authorization::Credentials;
    use headers::Authorization;
    use http::header::AUTHORIZATION;
    use http::{HeaderMap, Method, StatusCode, Uri};
    use similar_asserts::assert_eq;
    use vector_lib::codecs::{
        decoding::{DeserializerConfig, FramingConfig},
        BytesDecoderConfig, JsonDeserializerConfig,
    };
    use vector_lib::config::LogNamespace;
    use vector_lib::event::LogEvent;
    use vector_lib::lookup::lookup_v2::OptionalValuePath;
    use vector_lib::lookup::{event_path, owned_value_path, OwnedTargetPath, PathPrefix};
    use vector_lib::schema::Definition;
    use vrl::value::{kind::Collection, Kind, ObjectMap};

    use crate::common::http::server_auth::HttpServerAuthConfig;
    use crate::sources::http_server::HttpMethod;
    use crate::{
        components::validation::prelude::*,
        config::{log_schema, SourceConfig, SourceContext},
        event::{Event, EventStatus, Value},
        test_util::{
            components::{self, assert_source_compliance, HTTP_PUSH_SOURCE_TAGS},
            next_addr, spawn_collect_n, wait_for_tcp,
        },
        SourceSender,
    };

    use super::{remove_duplicates, SimpleHttpConfig};

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<SimpleHttpConfig>();
    }

    #[allow(clippy::too_many_arguments)]
    async fn source<'a>(
        headers: Vec<String>,
        query_parameters: Vec<String>,
        path_key: &'a str,
        host_key: &'a str,
        path: &'a str,
        method: &'a str,
        response_code: StatusCode,
        auth: Option<HttpServerAuthConfig>,
        strict_path: bool,
        status: EventStatus,
        acknowledgements: bool,
        framing: Option<FramingConfig>,
        decoding: Option<DeserializerConfig>,
    ) -> (impl Stream<Item = Event> + 'a, SocketAddr) {
        let (sender, recv) = SourceSender::new_test_finalize(status);
        let address = next_addr();
        let path = path.to_owned();
        let host_key = OptionalValuePath::from(owned_value_path!(host_key));
        let path_key = OptionalValuePath::from(owned_value_path!(path_key));
        let context = SourceContext::new_test(sender, None);
        let method = match Method::from_str(method).unwrap() {
            Method::GET => HttpMethod::Get,
            Method::POST => HttpMethod::Post,
            _ => HttpMethod::Post,
        };

        tokio::spawn(async move {
            SimpleHttpConfig {
                address,
                headers,
                encoding: None,
                query_parameters,
                response_code,
                tls: None,
                auth,
                strict_path,
                path_key,
                host_key,
                path,
                method,
                framing,
                decoding,
                acknowledgements: acknowledgements.into(),
                log_namespace: None,
                keepalive: Default::default(),
            }
            .build(context)
            .await
            .unwrap()
            .await
            .unwrap();
        });
        wait_for_tcp(address).await;
        (recv, address)
    }

    async fn send(address: SocketAddr, body: &str) -> u16 {
        reqwest::Client::new()
            .post(format!("http://{}/", address))
            .body(body.to_owned())
            .send()
            .await
            .unwrap()
            .status()
            .as_u16()
    }

    async fn send_with_headers(address: SocketAddr, body: &str, headers: HeaderMap) -> u16 {
        reqwest::Client::new()
            .post(format!("http://{}/", address))
            .headers(headers)
            .body(body.to_owned())
            .send()
            .await
            .unwrap()
            .status()
            .as_u16()
    }

    async fn send_with_query(address: SocketAddr, body: &str, query: &str) -> u16 {
        reqwest::Client::new()
            .post(format!("http://{}?{}", address, query))
            .body(body.to_owned())
            .send()
            .await
            .unwrap()
            .status()
            .as_u16()
    }

    async fn send_with_path(address: SocketAddr, body: &str, path: &str) -> u16 {
        reqwest::Client::new()
            .post(format!("http://{}{}", address, path))
            .body(body.to_owned())
            .send()
            .await
            .unwrap()
            .status()
            .as_u16()
    }

    async fn send_request(address: SocketAddr, method: &str, body: &str, path: &str) -> u16 {
        let method = Method::from_bytes(method.to_owned().as_bytes()).unwrap();
        reqwest::Client::new()
            .request(method, format!("http://{address}{path}"))
            .body(body.to_owned())
            .send()
            .await
            .unwrap()
            .status()
            .as_u16()
    }

    async fn send_bytes(address: SocketAddr, body: Vec<u8>, headers: HeaderMap) -> u16 {
        reqwest::Client::new()
            .post(format!("http://{address}/"))
            .headers(headers)
            .body(body)
            .send()
            .await
            .unwrap()
            .status()
            .as_u16()
    }

    async fn spawn_ok_collect_n(
        send: impl std::future::Future<Output = u16> + Send + 'static,
        rx: impl Stream<Item = Event> + Unpin,
        n: usize,
    ) -> Vec<Event> {
        spawn_collect_n(async move { assert_eq!(200, send.await) }, rx, n).await
    }

    #[tokio::test]
    async fn http_multiline_text() {
        let mut events = assert_source_compliance(&HTTP_PUSH_SOURCE_TAGS, async move {
            let body = "test body\ntest body 2";

            let (rx, addr) = source(
                vec![],
                vec![],
                "http_path",
                "remote_ip",
                "/",
                "POST",
                StatusCode::OK,
                None,
                true,
                EventStatus::Delivered,
                true,
                None,
                None,
            )
            .await;

            spawn_ok_collect_n(send(addr, body), rx, 2).await
        })
        .await;

        {
            let event = events.remove(0);
            let log = event.as_log();
            assert_eq!(*log.get_message().unwrap(), "test body".into());
            assert!(log.get_timestamp().is_some());
            assert_eq!(
                *log.get_source_type().unwrap(),
                SimpleHttpConfig::NAME.into()
            );
            assert_eq!(log["http_path"], "/".into());
            assert_event_metadata(log).await;
        }
        {
            let event = events.remove(0);
            let log = event.as_log();
            assert_eq!(*log.get_message().unwrap(), "test body 2".into());
            assert_event_metadata(log).await;
        }
    }

    #[tokio::test]
    async fn http_multiline_text2() {
        //same as above test but with a newline at the end
        let body = "test body\ntest body 2\n";

        let mut events = assert_source_compliance(&HTTP_PUSH_SOURCE_TAGS, async move {
            let (rx, addr) = source(
                vec![],
                vec![],
                "http_path",
                "remote_ip",
                "/",
                "POST",
                StatusCode::OK,
                None,
                true,
                EventStatus::Delivered,
                true,
                None,
                None,
            )
            .await;

            spawn_ok_collect_n(send(addr, body), rx, 2).await
        })
        .await;

        {
            let event = events.remove(0);
            let log = event.as_log();
            assert_eq!(*log.get_message().unwrap(), "test body".into());
            assert_event_metadata(log).await;
        }
        {
            let event = events.remove(0);
            let log = event.as_log();
            assert_eq!(*log.get_message().unwrap(), "test body 2".into());
            assert_event_metadata(log).await;
        }
    }

    #[tokio::test]
    async fn http_bytes_codec_preserves_newlines() {
        let body = "foo\nbar";

        let mut events = assert_source_compliance(&HTTP_PUSH_SOURCE_TAGS, async move {
            let (rx, addr) = source(
                vec![],
                vec![],
                "http_path",
                "remote_ip",
                "/",
                "POST",
                StatusCode::OK,
                None,
                true,
                EventStatus::Delivered,
                true,
                Some(BytesDecoderConfig::new().into()),
                None,
            )
            .await;

            spawn_ok_collect_n(send(addr, body), rx, 1).await
        })
        .await;

        assert_eq!(events.len(), 1);

        {
            let event = events.remove(0);
            let log = event.as_log();
            assert_eq!(*log.get_message().unwrap(), "foo\nbar".into());
            assert_event_metadata(log).await;
        }
    }

    #[tokio::test]
    async fn http_json_parsing() {
        let mut events = assert_source_compliance(&HTTP_PUSH_SOURCE_TAGS, async {
            let (rx, addr) = source(
                vec![],
                vec![],
                "http_path",
                "remote_ip",
                "/",
                "POST",
                StatusCode::OK,
                None,
                true,
                EventStatus::Delivered,
                true,
                None,
                Some(JsonDeserializerConfig::default().into()),
            )
            .await;

            spawn_collect_n(
                async move {
                    assert_eq!(400, send(addr, "{").await); //malformed
                    assert_eq!(400, send(addr, r#"{"key"}"#).await); //key without value

                    assert_eq!(200, send(addr, "{}").await); //can be one object or array of objects
                    assert_eq!(200, send(addr, "[{},{},{}]").await);
                },
                rx,
                2,
            )
            .await
        })
        .await;

        assert!(events.remove(1).as_log().get_timestamp().is_some());
        assert!(events.remove(0).as_log().get_timestamp().is_some());
    }

    #[tokio::test]
    async fn http_json_values() {
        let mut events = assert_source_compliance(&HTTP_PUSH_SOURCE_TAGS, async {
            let (rx, addr) = source(
                vec![],
                vec![],
                "http_path",
                "remote_ip",
                "/",
                "POST",
                StatusCode::OK,
                None,
                true,
                EventStatus::Delivered,
                true,
                None,
                Some(JsonDeserializerConfig::default().into()),
            )
            .await;

            spawn_collect_n(
                async move {
                    assert_eq!(200, send(addr, r#"[{"key":"value"}]"#).await);
                    assert_eq!(200, send(addr, r#"{"key2":"value2"}"#).await);
                },
                rx,
                2,
            )
            .await
        })
        .await;

        {
            let event = events.remove(0);
            let log = event.as_log();
            assert_eq!(log["key"], "value".into());
            assert_event_metadata(log).await;
        }
        {
            let event = events.remove(0);
            let log = event.as_log();
            assert_eq!(log["key2"], "value2".into());
            assert_event_metadata(log).await;
        }
    }

    #[tokio::test]
    async fn http_json_dotted_keys() {
        let mut events = assert_source_compliance(&HTTP_PUSH_SOURCE_TAGS, async {
            let (rx, addr) = source(
                vec![],
                vec![],
                "http_path",
                "remote_ip",
                "/",
                "POST",
                StatusCode::OK,
                None,
                true,
                EventStatus::Delivered,
                true,
                None,
                Some(JsonDeserializerConfig::default().into()),
            )
            .await;

            spawn_collect_n(
                async move {
                    assert_eq!(200, send(addr, r#"[{"dotted.key":"value"}]"#).await);
                    assert_eq!(
                        200,
                        send(addr, r#"{"nested":{"dotted.key2":"value2"}}"#).await
                    );
                },
                rx,
                2,
            )
            .await
        })
        .await;

        {
            let event = events.remove(0);
            let log = event.as_log();
            assert_eq!(
                log.get(event_path!("dotted.key")).unwrap(),
                &Value::from("value")
            );
        }
        {
            let event = events.remove(0);
            let log = event.as_log();
            let mut map = ObjectMap::new();
            map.insert("dotted.key2".into(), Value::from("value2"));
            assert_eq!(log["nested"], map.into());
        }
    }

    #[tokio::test]
    async fn http_ndjson() {
        let mut events = assert_source_compliance(&HTTP_PUSH_SOURCE_TAGS, async {
            let (rx, addr) = source(
                vec![],
                vec![],
                "http_path",
                "remote_ip",
                "/",
                "POST",
                StatusCode::OK,
                None,
                true,
                EventStatus::Delivered,
                true,
                None,
                Some(JsonDeserializerConfig::default().into()),
            )
            .await;

            spawn_collect_n(
                async move {
                    assert_eq!(
                        200,
                        send(addr, r#"[{"key1":"value1"},{"key2":"value2"}]"#).await
                    );

                    assert_eq!(
                        200,
                        send(addr, "{\"key1\":\"value1\"}\n\n{\"key2\":\"value2\"}").await
                    );
                },
                rx,
                4,
            )
            .await
        })
        .await;

        {
            let event = events.remove(0);
            let log = event.as_log();
            assert_eq!(log["key1"], "value1".into());
            assert_event_metadata(log).await;
        }
        {
            let event = events.remove(0);
            let log = event.as_log();
            assert_eq!(log["key2"], "value2".into());
            assert_event_metadata(log).await;
        }
        {
            let event = events.remove(0);
            let log = event.as_log();
            assert_eq!(log["key1"], "value1".into());
            assert_event_metadata(log).await;
        }
        {
            let event = events.remove(0);
            let log = event.as_log();
            assert_eq!(log["key2"], "value2".into());
            assert_event_metadata(log).await;
        }
    }

    async fn assert_event_metadata(log: &LogEvent) {
        assert!(log.get_timestamp().is_some());

        let source_type_key_value = log
            .get((PathPrefix::Event, log_schema().source_type_key().unwrap()))
            .unwrap()
            .as_str()
            .unwrap();
        assert_eq!(source_type_key_value, SimpleHttpConfig::NAME);
        assert_eq!(log["http_path"], "/".into());
    }

    #[tokio::test]
    async fn http_headers() {
        let mut events = assert_source_compliance(&HTTP_PUSH_SOURCE_TAGS, async {
            let mut headers = HeaderMap::new();
            headers.insert("User-Agent", "test_client".parse().unwrap());
            headers.insert("Upgrade-Insecure-Requests", "false".parse().unwrap());
            headers.insert("X-Test-Header", "true".parse().unwrap());

            let (rx, addr) = source(
                vec![
                    "User-Agent".to_string(),
                    "Upgrade-Insecure-Requests".to_string(),
                    "X-*".to_string(),
                    "AbsentHeader".to_string(),
                ],
                vec![],
                "http_path",
                "remote_ip",
                "/",
                "POST",
                StatusCode::OK,
                None,
                true,
                EventStatus::Delivered,
                true,
                None,
                Some(JsonDeserializerConfig::default().into()),
            )
            .await;

            spawn_ok_collect_n(
                send_with_headers(addr, "{\"key1\":\"value1\"}", headers),
                rx,
                1,
            )
            .await
        })
        .await;

        {
            let event = events.remove(0);
            let log = event.as_log();
            assert_eq!(log["key1"], "value1".into());
            assert_eq!(log["\"User-Agent\""], "test_client".into());
            assert_eq!(log["\"Upgrade-Insecure-Requests\""], "false".into());
            assert_eq!(log["\"x-test-header\""], "true".into());
            assert_eq!(log["AbsentHeader"], Value::Null);
            assert_event_metadata(log).await;
        }
    }

    #[tokio::test]
    async fn http_headers_wildcard() {
        let mut events = assert_source_compliance(&HTTP_PUSH_SOURCE_TAGS, async {
            let mut headers = HeaderMap::new();
            headers.insert("User-Agent", "test_client".parse().unwrap());
            headers.insert("X-Case-Sensitive-Value", "CaseSensitive".parse().unwrap());
            // Header that conflicts with an existing field.
            headers.insert("key1", "value_from_header".parse().unwrap());

            let (rx, addr) = source(
                vec!["*".to_string()],
                vec![],
                "http_path",
                "remote_ip",
                "/",
                "POST",
                StatusCode::OK,
                None,
                true,
                EventStatus::Delivered,
                true,
                None,
                Some(JsonDeserializerConfig::default().into()),
            )
            .await;

            spawn_ok_collect_n(
                send_with_headers(addr, "{\"key1\":\"value1\"}", headers),
                rx,
                1,
            )
            .await
        })
        .await;

        {
            let event = events.remove(0);
            let log = event.as_log();
            assert_eq!(log["key1"], "value1".into());
            assert_eq!(log["\"user-agent\""], "test_client".into());
            assert_eq!(log["\"x-case-sensitive-value\""], "CaseSensitive".into());
            assert_event_metadata(log).await;
        }
    }

    #[tokio::test]
    async fn http_query() {
        let mut events = assert_source_compliance(&HTTP_PUSH_SOURCE_TAGS, async {
            let (rx, addr) = source(
                vec![],
                vec![
                    "source".to_string(),
                    "region".to_string(),
                    "absent".to_string(),
                ],
                "http_path",
                "remote_ip",
                "/",
                "POST",
                StatusCode::OK,
                None,
                true,
                EventStatus::Delivered,
                true,
                None,
                Some(JsonDeserializerConfig::default().into()),
            )
            .await;

            spawn_ok_collect_n(
                send_with_query(addr, "{\"key1\":\"value1\"}", "source=staging&region=gb"),
                rx,
                1,
            )
            .await
        })
        .await;

        {
            let event = events.remove(0);
            let log = event.as_log();
            assert_eq!(log["key1"], "value1".into());
            assert_eq!(log["source"], "staging".into());
            assert_eq!(log["region"], "gb".into());
            assert_eq!(log["absent"], Value::Null);
            assert_event_metadata(log).await;
        }
    }

    #[tokio::test]
    async fn http_query_wildcard() {
        let mut events = assert_source_compliance(&HTTP_PUSH_SOURCE_TAGS, async {
            let (rx, addr) = source(
                vec![],
                vec!["*".to_string()],
                "http_path",
                "remote_ip",
                "/",
                "POST",
                StatusCode::OK,
                None,
                true,
                EventStatus::Delivered,
                true,
                None,
                Some(JsonDeserializerConfig::default().into()),
            )
            .await;

            spawn_ok_collect_n(
                send_with_query(
                    addr,
                    "{\"key1\":\"value1\",\"key2\":\"value2\"}",
                    "source=staging&region=gb&key1=value_from_query",
                ),
                rx,
                1,
            )
            .await
        })
        .await;

        {
            let event = events.remove(0);
            let log = event.as_log();
            assert_eq!(log["key1"], "value_from_query".into());
            assert_eq!(log["key2"], "value2".into());
            assert_eq!(log["source"], "staging".into());
            assert_eq!(log["region"], "gb".into());
            assert_event_metadata(log).await;
        }
    }

    #[tokio::test]
    async fn http_gzip_deflate() {
        let mut events = assert_source_compliance(&HTTP_PUSH_SOURCE_TAGS, async {
            let body = "test body";

            let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
            encoder.write_all(body.as_bytes()).unwrap();
            let body = encoder.finish().unwrap();

            let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
            encoder.write_all(body.as_slice()).unwrap();
            let body = encoder.finish().unwrap();

            let mut headers = HeaderMap::new();
            headers.insert("Content-Encoding", "gzip, deflate".parse().unwrap());

            let (rx, addr) = source(
                vec![],
                vec![],
                "http_path",
                "remote_ip",
                "/",
                "POST",
                StatusCode::OK,
                None,
                true,
                EventStatus::Delivered,
                true,
                None,
                None,
            )
            .await;

            spawn_ok_collect_n(send_bytes(addr, body, headers), rx, 1).await
        })
        .await;

        {
            let event = events.remove(0);
            let log = event.as_log();
            assert_eq!(*log.get_message().unwrap(), "test body".into());
            assert_event_metadata(log).await;
        }
    }

    #[tokio::test]
    async fn http_path() {
        let mut events = assert_source_compliance(&HTTP_PUSH_SOURCE_TAGS, async {
            let (rx, addr) = source(
                vec![],
                vec![],
                "vector_http_path",
                "vector_remote_ip",
                "/event/path",
                "POST",
                StatusCode::OK,
                None,
                true,
                EventStatus::Delivered,
                true,
                None,
                Some(JsonDeserializerConfig::default().into()),
            )
            .await;

            spawn_ok_collect_n(
                send_with_path(addr, "{\"key1\":\"value1\"}", "/event/path"),
                rx,
                1,
            )
            .await
        })
        .await;

        {
            let event = events.remove(0);
            let log = event.as_log();
            assert_eq!(log["key1"], "value1".into());
            assert_eq!(log["vector_http_path"], "/event/path".into());
            assert!(log.get_timestamp().is_some());
            assert_eq!(
                *log.get_source_type().unwrap(),
                SimpleHttpConfig::NAME.into()
            );
        }
    }

    #[tokio::test]
    async fn http_path_no_restriction() {
        let mut events = assert_source_compliance(&HTTP_PUSH_SOURCE_TAGS, async {
            let (rx, addr) = source(
                vec![],
                vec![],
                "vector_http_path",
                "vector_remote_ip",
                "/event",
                "POST",
                StatusCode::OK,
                None,
                false,
                EventStatus::Delivered,
                true,
                None,
                Some(JsonDeserializerConfig::default().into()),
            )
            .await;

            spawn_collect_n(
                async move {
                    assert_eq!(
                        200,
                        send_with_path(addr, "{\"key1\":\"value1\"}", "/event/path1").await
                    );
                    assert_eq!(
                        200,
                        send_with_path(addr, "{\"key2\":\"value2\"}", "/event/path2").await
                    );
                },
                rx,
                2,
            )
            .await
        })
        .await;

        {
            let event = events.remove(0);
            let log = event.as_log();
            assert_eq!(log["key1"], "value1".into());
            assert_eq!(log["vector_http_path"], "/event/path1".into());
            assert!(log.get_timestamp().is_some());
            assert_eq!(
                *log.get_source_type().unwrap(),
                SimpleHttpConfig::NAME.into()
            );
        }
        {
            let event = events.remove(0);
            let log = event.as_log();
            assert_eq!(log["key2"], "value2".into());
            assert_eq!(log["vector_http_path"], "/event/path2".into());
            assert!(log.get_timestamp().is_some());
            assert_eq!(
                *log.get_source_type().unwrap(),
                SimpleHttpConfig::NAME.into()
            );
        }
    }

    #[tokio::test]
    async fn http_wrong_path() {
        components::init_test();
        let (_rx, addr) = source(
            vec![],
            vec![],
            "vector_http_path",
            "vector_remote_ip",
            "/",
            "POST",
            StatusCode::OK,
            None,
            true,
            EventStatus::Delivered,
            true,
            None,
            Some(JsonDeserializerConfig::default().into()),
        )
        .await;

        assert_eq!(
            404,
            send_with_path(addr, "{\"key1\":\"value1\"}", "/event/path").await
        );
    }

    #[tokio::test]
    async fn http_status_code() {
        assert_source_compliance(&HTTP_PUSH_SOURCE_TAGS, async move {
            let (rx, addr) = source(
                vec![],
                vec![],
                "http_path",
                "remote_ip",
                "/",
                "POST",
                StatusCode::ACCEPTED,
                None,
                true,
                EventStatus::Delivered,
                true,
                None,
                None,
            )
            .await;

            spawn_collect_n(
                async move {
                    assert_eq!(
                        StatusCode::ACCEPTED,
                        send(addr, "{\"key1\":\"value1\"}").await
                    );
                },
                rx,
                1,
            )
            .await;
        })
        .await;
    }

    #[tokio::test]
    async fn http_delivery_failure() {
        assert_source_compliance(&HTTP_PUSH_SOURCE_TAGS, async {
            let (rx, addr) = source(
                vec![],
                vec![],
                "http_path",
                "remote_ip",
                "/",
                "POST",
                StatusCode::OK,
                None,
                true,
                EventStatus::Rejected,
                true,
                None,
                None,
            )
            .await;

            spawn_collect_n(
                async move {
                    assert_eq!(400, send(addr, "test body\n").await);
                },
                rx,
                1,
            )
            .await;
        })
        .await;
    }

    #[tokio::test]
    async fn ignores_disabled_acknowledgements() {
        let events = assert_source_compliance(&HTTP_PUSH_SOURCE_TAGS, async {
            let (rx, addr) = source(
                vec![],
                vec![],
                "http_path",
                "remote_ip",
                "/",
                "POST",
                StatusCode::OK,
                None,
                true,
                EventStatus::Rejected,
                false,
                None,
                None,
            )
            .await;

            spawn_collect_n(
                async move {
                    assert_eq!(200, send(addr, "test body\n").await);
                },
                rx,
                1,
            )
            .await
        })
        .await;

        assert_eq!(events.len(), 1);
    }

    #[tokio::test]
    async fn http_get_method() {
        components::init_test();
        let (_rx, addr) = source(
            vec![],
            vec![],
            "http_path",
            "remote_ip",
            "/",
            "GET",
            StatusCode::OK,
            None,
            true,
            EventStatus::Delivered,
            true,
            None,
            None,
        )
        .await;

        assert_eq!(200, send_request(addr, "GET", "", "/").await);
    }

    #[tokio::test]
    async fn returns_401_when_required_auth_is_missing() {
        components::init_test();
        let (_rx, addr) = source(
            vec![],
            vec![],
            "http_path",
            "remote_ip",
            "/",
            "GET",
            StatusCode::OK,
            Some(HttpServerAuthConfig::Basic {
                username: "test".to_string(),
                password: "test".to_string().into(),
            }),
            true,
            EventStatus::Delivered,
            true,
            None,
            None,
        )
        .await;

        assert_eq!(401, send_request(addr, "GET", "", "/").await);
    }

    #[tokio::test]
    async fn returns_401_when_required_auth_is_wrong() {
        components::init_test();
        let (_rx, addr) = source(
            vec![],
            vec![],
            "http_path",
            "remote_ip",
            "/",
            "POST",
            StatusCode::OK,
            Some(HttpServerAuthConfig::Basic {
                username: "test".to_string(),
                password: "test".to_string().into(),
            }),
            true,
            EventStatus::Delivered,
            true,
            None,
            None,
        )
        .await;

        let mut headers = HeaderMap::new();
        headers.insert(
            AUTHORIZATION,
            Authorization::basic("wrong", "test").0.encode(),
        );
        assert_eq!(401, send_with_headers(addr, "", headers).await);
    }

    #[tokio::test]
    async fn http_get_with_correct_auth() {
        components::init_test();
        let (_rx, addr) = source(
            vec![],
            vec![],
            "http_path",
            "remote_ip",
            "/",
            "POST",
            StatusCode::OK,
            Some(HttpServerAuthConfig::Basic {
                username: "test".to_string(),
                password: "test".to_string().into(),
            }),
            true,
            EventStatus::Delivered,
            true,
            None,
            None,
        )
        .await;

        let mut headers = HeaderMap::new();
        headers.insert(
            AUTHORIZATION,
            Authorization::basic("test", "test").0.encode(),
        );
        assert_eq!(200, send_with_headers(addr, "", headers).await);
    }

    #[test]
    fn output_schema_definition_vector_namespace() {
        let config = SimpleHttpConfig {
            log_namespace: Some(true),
            ..Default::default()
        };

        let definitions = config
            .outputs(LogNamespace::Vector)
            .remove(0)
            .schema_definition(true);

        let expected_definition =
            Definition::new_with_default_metadata(Kind::bytes(), [LogNamespace::Vector])
                .with_meaning(OwnedTargetPath::event_root(), "message")
                .with_metadata_field(
                    &owned_value_path!("vector", "source_type"),
                    Kind::bytes(),
                    None,
                )
                .with_metadata_field(
                    &owned_value_path!(SimpleHttpConfig::NAME, "path"),
                    Kind::bytes(),
                    None,
                )
                .with_metadata_field(
                    &owned_value_path!(SimpleHttpConfig::NAME, "headers"),
                    Kind::object(Collection::empty().with_unknown(Kind::bytes())).or_undefined(),
                    None,
                )
                .with_metadata_field(
                    &owned_value_path!(SimpleHttpConfig::NAME, "query_parameters"),
                    Kind::object(Collection::empty().with_unknown(Kind::bytes())).or_undefined(),
                    None,
                )
                .with_metadata_field(
                    &owned_value_path!(SimpleHttpConfig::NAME, "host"),
                    Kind::bytes().or_undefined(),
                    None,
                )
                .with_metadata_field(
                    &owned_value_path!("vector", "ingest_timestamp"),
                    Kind::timestamp(),
                    None,
                );

        assert_eq!(definitions, Some(expected_definition))
    }

    #[test]
    fn output_schema_definition_legacy_namespace() {
        let config = SimpleHttpConfig::default();

        let definitions = config
            .outputs(LogNamespace::Legacy)
            .remove(0)
            .schema_definition(true);

        let expected_definition = Definition::new_with_default_metadata(
            Kind::object(Collection::empty()),
            [LogNamespace::Legacy],
        )
        .with_event_field(
            &owned_value_path!("message"),
            Kind::bytes(),
            Some("message"),
        )
        .with_event_field(&owned_value_path!("source_type"), Kind::bytes(), None)
        .with_event_field(&owned_value_path!("timestamp"), Kind::timestamp(), None)
        .with_event_field(&owned_value_path!("path"), Kind::bytes(), None)
        .with_event_field(
            &owned_value_path!("host"),
            Kind::bytes().or_undefined(),
            None,
        )
        .unknown_fields(Kind::bytes());

        assert_eq!(definitions, Some(expected_definition))
    }

    #[test]
    fn validate_remove_duplicates() {
        let mut list = vec![
            "a".to_owned(),
            "b".to_owned(),
            "c".to_owned(),
            "d".to_owned(),
        ];

        // no duplicates should be identical
        {
            let list_dedup = remove_duplicates(list.clone(), "foo");

            assert_eq!(list, list_dedup);
        }

        list.push("b".to_owned());

        // remove duplicate "b"
        {
            let list_dedup = remove_duplicates(list.clone(), "foo");
            assert_eq!(
                vec![
                    "a".to_owned(),
                    "b".to_owned(),
                    "c".to_owned(),
                    "d".to_owned()
                ],
                list_dedup
            );
        }
    }

    impl ValidatableComponent for SimpleHttpConfig {
        fn validation_configuration() -> ValidationConfiguration {
            let config = Self {
                decoding: Some(DeserializerConfig::Json(Default::default())),
                ..Default::default()
            };

            let log_namespace: LogNamespace = config.log_namespace.unwrap_or(false).into();

            let listen_addr_http = format!("http://{}/", config.address);
            let uri = Uri::try_from(&listen_addr_http).expect("should not fail to parse URI");

            let external_resource = ExternalResource::new(
                ResourceDirection::Push,
                HttpResourceConfig::from_parts(uri, Some(config.method.into())),
                config
                    .get_decoding_config()
                    .expect("should not fail to get decoding config"),
            );

            ValidationConfiguration::from_source(
                Self::NAME,
                log_namespace,
                vec![ComponentTestCaseConfig::from_source(
                    config,
                    None,
                    Some(external_resource),
                )],
            )
        }
    }

    register_validatable_component!(SimpleHttpConfig);
}
