use std::{collections::HashMap, net::SocketAddr};

use bytes::{Bytes, BytesMut};
use chrono::Utc;
use codecs::{
    decoding::{DeserializerConfig, FramingConfig},
    BytesDecoderConfig, BytesDeserializerConfig, JsonDeserializerConfig,
    NewlineDelimitedDecoderConfig,
};

use http::{StatusCode, Uri};
use lookup::{lookup_v2::OptionalValuePath, owned_value_path, path};
use tokio_util::codec::Decoder as _;
use value::{kind::Collection, Kind};
use vector_config::{configurable_component, NamedComponent};
use vector_core::{
    config::{DataType, LegacyKey, LogNamespace},
    schema::Definition,
};
use warp::http::{HeaderMap, HeaderValue};

use crate::{
    codecs::{Decoder, DecodingConfig},
    components::validation::*,
    config::{
        GenerateConfig, Output, Resource, SourceAcknowledgementsConfig, SourceConfig, SourceContext,
    },
    event::{Event, Value},
    register_validatable_component,
    serde::{bool_or_struct, default_decoding},
    sources::util::{
        http::{add_query_parameters, HttpMethod},
        Encoding, ErrorMessage, HttpSource, HttpSourceAuthConfig,
    },
    tls::TlsEnableableConfig,
};

/// Configuration for the `http` source.
#[configurable_component(source("http"))]
#[configurable(metadata(deprecated))]
#[derive(Clone, Debug)]
pub struct HttpConfig(SimpleHttpConfig);

impl GenerateConfig for HttpConfig {
    fn generate_config() -> toml::Value {
        <SimpleHttpConfig as GenerateConfig>::generate_config()
    }
}

#[async_trait::async_trait]
impl SourceConfig for HttpConfig {
    async fn build(&self, cx: SourceContext) -> vector_common::Result<super::Source> {
        self.0.build(cx).await
    }

    fn outputs(&self, global_log_namespace: LogNamespace) -> Vec<Output> {
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
#[configurable_component(source("http_server"))]
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
    /// Note that for `json` and `ndjson` encodings, the fields of the JSON objects are output as separate fields.
    #[serde(default)]
    encoding: Option<Encoding>,

    /// A list of HTTP headers to include in the log event.
    ///
    /// These will override any values included in the JSON payload with conflicting names.
    #[serde(default)]
    #[configurable(metadata(docs::examples = "User-Agent"))]
    #[configurable(metadata(docs::examples = "X-My-Custom-Header"))]
    headers: Vec<String>,

    /// A list of URL query parameters to include in the log event.
    ///
    /// These will override any values included in the body with conflicting names.
    #[serde(default)]
    #[configurable(metadata(docs::examples = "application"))]
    #[configurable(metadata(docs::examples = "source"))]
    query_parameters: Vec<String>,

    #[configurable(derived)]
    auth: Option<HttpSourceAuthConfig>,

    /// Whether or not to treat the configured `path` as an absolute path.
    ///
    /// If set to `true`, only requests using the exact URL path specified in `path` will be accepted. Otherwise,
    /// requests sent to a URL path that starts with the value of `path` will be accepted.
    ///
    /// With `strict_path` set to `false` and `path` set to `""`, the configured HTTP source will accept requests from
    /// any URL path.
    #[serde(default = "crate::serde::default_true")]
    strict_path: bool,

    /// The URL path on which log event POST requests shall be sent.
    #[serde(default = "default_path")]
    #[configurable(metadata(docs::examples = "/event/path"))]
    #[configurable(metadata(docs::examples = "/logs"))]
    path: String,

    /// The event key in which the requested URL path used to send the request will be stored.
    #[serde(default = "default_path_key")]
    #[configurable(metadata(docs::examples = "vector_http_path"))]
    path_key: OptionalValuePath,

    /// Specifies the action of the HTTP request.
    #[serde(default = "default_http_method")]
    method: HttpMethod,

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
                    JsonDeserializerConfig::new().into(),
                ),
                Encoding::Ndjson => (
                    NewlineDelimitedDecoderConfig::new().into(),
                    JsonDeserializerConfig::new().into(),
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
            method: default_http_method(),
            strict_path: true,
            framing: None,
            decoding: Some(default_decoding()),
            acknowledgements: SourceAcknowledgementsConfig::default(),
            log_namespace: None,
        }
    }
}

impl_generate_config_from_default!(SimpleHttpConfig);

impl ValidatableComponent for SimpleHttpConfig {
    fn validation_configuration() -> ValidationConfiguration {
        let config = Self {
            decoding: Some(DeserializerConfig::Json),
            ..Default::default()
        };

        let listen_addr_http = format!("http://{}/", config.address);
        let uri = Uri::try_from(&listen_addr_http).expect("should not fail to parse URI");

        let external_resource = ExternalResource::new(
            ResourceDirection::Push,
            HttpResourceConfig::from_parts(uri, Some(config.method.into())),
            config
                .get_decoding_config()
                .expect("should not fail to get decoding config"),
        );

        ValidationConfiguration::from_source(Self::NAME, config, Some(external_resource))
    }
}

register_validatable_component!(SimpleHttpConfig);

const fn default_http_method() -> HttpMethod {
    HttpMethod::Post
}

fn default_path() -> String {
    "/".to_string()
}

fn default_path_key() -> OptionalValuePath {
    OptionalValuePath::from(owned_value_path!("path"))
}

/// Removes duplicates from the list, and logs a `warn!()` for each duplicate removed.
fn remove_duplicates(mut list: Vec<String>, list_name: &str) -> Vec<String> {
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

#[async_trait::async_trait]
impl SourceConfig for SimpleHttpConfig {
    async fn build(&self, cx: SourceContext) -> crate::Result<super::Source> {
        let decoder = self.get_decoding_config()?.build();
        let log_namespace = cx.log_namespace(self.log_namespace);

        let source = SimpleHttpSource {
            headers: remove_duplicates(self.headers.clone(), "headers"),
            query_parameters: remove_duplicates(self.query_parameters.clone(), "query_parameters"),
            path_key: self.path_key.clone(),
            decoder,
            log_namespace,
        };
        source.run(
            self.address,
            self.path.as_str(),
            self.method,
            self.strict_path,
            &self.tls,
            &self.auth,
            cx,
            self.acknowledgements,
        )
    }

    fn outputs(&self, global_log_namespace: LogNamespace) -> Vec<Output> {
        // There is a global and per-source `log_namespace` config.
        // The source config overrides the global setting and is merged here.
        let log_namespace = global_log_namespace.merge(self.log_namespace);

        let schema_definition = self.schema_definition(log_namespace);

        vec![Output::default(
            self.decoding
                .as_ref()
                .map(|d| d.output_type())
                .unwrap_or(DataType::Log),
        )
        .with_schema_definition(schema_definition)]
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
    headers: Vec<String>,
    query_parameters: Vec<String>,
    path_key: OptionalValuePath,
    decoder: Decoder,
    log_namespace: LogNamespace,
}

impl SimpleHttpSource {
    /// Enriches the passed in events with metadata for the `request_path` and for each of the headers.
    fn enrich_events(
        &self,
        events: &mut [Event],
        request_path: &str,
        headers_config: HeaderMap,
        query_parameters: HashMap<String, String>,
    ) {
        for event in events.iter_mut() {
            let log = event.as_mut_log();

            // add request_path to each event
            self.log_namespace.insert_source_metadata(
                SimpleHttpConfig::NAME,
                log,
                self.path_key.path.as_ref().map(LegacyKey::InsertIfEmpty),
                path!("path"),
                request_path.to_owned(),
            );

            // add each header to each event
            for header_name in &self.headers {
                let value = headers_config.get(header_name).map(HeaderValue::as_bytes);

                self.log_namespace.insert_source_metadata(
                    SimpleHttpConfig::NAME,
                    log,
                    Some(LegacyKey::InsertIfEmpty(path!(header_name))),
                    path!("headers", header_name),
                    Value::from(value.map(Bytes::copy_from_slice)),
                );
            }
        }

        add_query_parameters(
            events,
            &self.query_parameters,
            query_parameters,
            self.log_namespace,
            SimpleHttpConfig::NAME,
        );

        let now = Utc::now();
        for event in events {
            let log = event.as_mut_log();

            self.log_namespace.insert_standard_vector_source_metadata(
                log,
                SimpleHttpConfig::NAME,
                now,
            );
        }
    }
}

impl HttpSource for SimpleHttpSource {
    fn build_events(
        &self,
        body: Bytes,
        header_map: HeaderMap,
        query_parameters: HashMap<String, String>,
        request_path: &str,
    ) -> Result<Vec<Event>, ErrorMessage> {
        let mut decoder = self.decoder.clone();
        let mut events = Vec::new();
        let mut bytes = BytesMut::new();
        bytes.extend_from_slice(&body);

        loop {
            match decoder.decode_eof(&mut bytes) {
                Ok(Some((next, _))) => {
                    events.extend(next.into_iter());
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

        self.enrich_events(&mut events, request_path, header_map, query_parameters);

        Ok(events)
    }
}

#[cfg(test)]
mod tests {
    use lookup::{event_path, owned_value_path, LookupBuf};
    use std::str::FromStr;
    use std::{collections::BTreeMap, io::Write, net::SocketAddr};
    use value::kind::Collection;
    use value::Kind;
    use vector_config::NamedComponent;
    use vector_core::config::LogNamespace;
    use vector_core::event::LogEvent;
    use vector_core::schema::Definition;

    use codecs::{
        decoding::{DeserializerConfig, FramingConfig},
        BytesDecoderConfig, JsonDeserializerConfig,
    };
    use flate2::{
        write::{GzEncoder, ZlibEncoder},
        Compression,
    };
    use futures::Stream;
    use http::{HeaderMap, Method};
    use lookup::lookup_v2::OptionalValuePath;
    use similar_asserts::assert_eq;

    use super::{remove_duplicates, SimpleHttpConfig};
    use crate::sources::http_server::HttpMethod;
    use crate::{
        config::{log_schema, SourceConfig, SourceContext},
        event::{Event, EventStatus, Value},
        test_util::{
            components::{self, assert_source_compliance, HTTP_PUSH_SOURCE_TAGS},
            next_addr, spawn_collect_n, wait_for_tcp,
        },
        SourceSender,
    };

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<SimpleHttpConfig>();
    }

    #[allow(clippy::too_many_arguments)]
    async fn source<'a>(
        headers: Vec<String>,
        query_parameters: Vec<String>,
        path_key: &'a str,
        path: &'a str,
        method: &'a str,
        strict_path: bool,
        status: EventStatus,
        acknowledgements: bool,
        framing: Option<FramingConfig>,
        decoding: Option<DeserializerConfig>,
    ) -> (impl Stream<Item = Event> + 'a, SocketAddr) {
        let (sender, recv) = SourceSender::new_test_finalize(status);
        let address = next_addr();
        let path = path.to_owned();
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
                tls: None,
                auth: None,
                strict_path,
                path_key,
                path,
                method,
                framing,
                decoding,
                acknowledgements: acknowledgements.into(),
                log_namespace: None,
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
            .post(&format!("http://{}/", address))
            .body(body.to_owned())
            .send()
            .await
            .unwrap()
            .status()
            .as_u16()
    }

    async fn send_with_headers(address: SocketAddr, body: &str, headers: HeaderMap) -> u16 {
        reqwest::Client::new()
            .post(&format!("http://{}/", address))
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
            .post(&format!("http://{}?{}", address, query))
            .body(body.to_owned())
            .send()
            .await
            .unwrap()
            .status()
            .as_u16()
    }

    async fn send_with_path(address: SocketAddr, body: &str, path: &str) -> u16 {
        reqwest::Client::new()
            .post(&format!("http://{}{}", address, path))
            .body(body.to_owned())
            .send()
            .await
            .unwrap()
            .status()
            .as_u16()
    }

    async fn send_request(address: SocketAddr, method: &str, body: &str, path: &str) -> u16 {
        let method = Method::from_bytes(method.to_owned().as_bytes()).unwrap();
        format!("method: {}", method.as_str());
        reqwest::Client::new()
            .request(method, &format!("http://{}{}", address, path))
            .body(body.to_owned())
            .send()
            .await
            .unwrap()
            .status()
            .as_u16()
    }

    async fn send_bytes(address: SocketAddr, body: Vec<u8>, headers: HeaderMap) -> u16 {
        reqwest::Client::new()
            .post(&format!("http://{}/", address))
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
                "/",
                "POST",
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
            assert_eq!(log[log_schema().message_key()], "test body".into());
            assert!(log.get(log_schema().timestamp_key()).is_some());
            assert_eq!(
                log[log_schema().source_type_key()],
                SimpleHttpConfig::NAME.into()
            );
            assert_eq!(log["http_path"], "/".into());
            assert_event_metadata(log).await;
        }
        {
            let event = events.remove(0);
            let log = event.as_log();
            assert_eq!(log[log_schema().message_key()], "test body 2".into());
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
                "/",
                "POST",
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
            assert_eq!(log[log_schema().message_key()], "test body".into());
            assert_event_metadata(log).await;
        }
        {
            let event = events.remove(0);
            let log = event.as_log();
            assert_eq!(log[log_schema().message_key()], "test body 2".into());
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
                "/",
                "POST",
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
            assert_eq!(log[log_schema().message_key()], "foo\nbar".into());
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
                "/",
                "POST",
                true,
                EventStatus::Delivered,
                true,
                None,
                Some(JsonDeserializerConfig::new().into()),
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

        assert!(events
            .remove(1)
            .as_log()
            .get(log_schema().timestamp_key())
            .is_some());
        assert!(events
            .remove(0)
            .as_log()
            .get(log_schema().timestamp_key())
            .is_some());
    }

    #[tokio::test]
    async fn http_json_values() {
        let mut events = assert_source_compliance(&HTTP_PUSH_SOURCE_TAGS, async {
            let (rx, addr) = source(
                vec![],
                vec![],
                "http_path",
                "/",
                "POST",
                true,
                EventStatus::Delivered,
                true,
                None,
                Some(JsonDeserializerConfig::new().into()),
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
                "/",
                "POST",
                true,
                EventStatus::Delivered,
                true,
                None,
                Some(JsonDeserializerConfig::new().into()),
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
            let mut map = BTreeMap::new();
            map.insert("dotted.key2".to_string(), Value::from("value2"));
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
                "/",
                "POST",
                true,
                EventStatus::Delivered,
                true,
                None,
                Some(JsonDeserializerConfig::new().into()),
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
        assert!(log.get(log_schema().timestamp_key()).is_some());
        assert_eq!(
            log[log_schema().source_type_key()],
            SimpleHttpConfig::NAME.into()
        );
        assert_eq!(log["http_path"], "/".into());
    }

    #[tokio::test]
    async fn http_headers() {
        let mut events = assert_source_compliance(&HTTP_PUSH_SOURCE_TAGS, async {
            let mut headers = HeaderMap::new();
            headers.insert("User-Agent", "test_client".parse().unwrap());
            headers.insert("Upgrade-Insecure-Requests", "false".parse().unwrap());

            let (rx, addr) = source(
                vec![
                    "User-Agent".to_string(),
                    "Upgrade-Insecure-Requests".to_string(),
                    "AbsentHeader".to_string(),
                ],
                vec![],
                "http_path",
                "/",
                "POST",
                true,
                EventStatus::Delivered,
                true,
                None,
                Some(JsonDeserializerConfig::new().into()),
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
            assert_eq!(log["AbsentHeader"], Value::Null);
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
                "/",
                "POST",
                true,
                EventStatus::Delivered,
                true,
                None,
                Some(JsonDeserializerConfig::new().into()),
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
                "/",
                "POST",
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
            assert_eq!(log[log_schema().message_key()], "test body".into());
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
                "/event/path",
                "POST",
                true,
                EventStatus::Delivered,
                true,
                None,
                Some(JsonDeserializerConfig::new().into()),
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
            assert!(log.get(log_schema().timestamp_key()).is_some());
            assert_eq!(
                log[log_schema().source_type_key()],
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
                "/event",
                "POST",
                false,
                EventStatus::Delivered,
                true,
                None,
                Some(JsonDeserializerConfig::new().into()),
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
            assert!(log.get(log_schema().timestamp_key()).is_some());
            assert_eq!(
                log[log_schema().source_type_key()],
                SimpleHttpConfig::NAME.into()
            );
        }
        {
            let event = events.remove(0);
            let log = event.as_log();
            assert_eq!(log["key2"], "value2".into());
            assert_eq!(log["vector_http_path"], "/event/path2".into());
            assert!(log.get(log_schema().timestamp_key()).is_some());
            assert_eq!(
                log[log_schema().source_type_key()],
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
            "/",
            "POST",
            true,
            EventStatus::Delivered,
            true,
            None,
            Some(JsonDeserializerConfig::new().into()),
        )
        .await;

        assert_eq!(
            404,
            send_with_path(addr, "{\"key1\":\"value1\"}", "/event/path").await
        );
    }

    #[tokio::test]
    async fn http_delivery_failure() {
        assert_source_compliance(&HTTP_PUSH_SOURCE_TAGS, async {
            let (rx, addr) = source(
                vec![],
                vec![],
                "http_path",
                "/",
                "POST",
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
                "/",
                "POST",
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
            "/",
            "GET",
            true,
            EventStatus::Delivered,
            true,
            None,
            None,
        )
        .await;

        assert_eq!(200, send_request(addr, "GET", "", "/").await);
    }

    #[test]
    fn output_schema_definition_vector_namespace() {
        let config = SimpleHttpConfig {
            log_namespace: Some(true),
            ..Default::default()
        };

        let definition = config.outputs(LogNamespace::Vector)[0]
            .clone()
            .log_schema_definition
            .unwrap();

        let expected_definition =
            Definition::new_with_default_metadata(Kind::bytes(), [LogNamespace::Vector])
                .with_meaning(LookupBuf::root(), "message")
                .with_metadata_field(&owned_value_path!("vector", "source_type"), Kind::bytes())
                .with_metadata_field(
                    &owned_value_path!(SimpleHttpConfig::NAME, "path"),
                    Kind::bytes(),
                )
                .with_metadata_field(
                    &owned_value_path!(SimpleHttpConfig::NAME, "headers"),
                    Kind::object(Collection::empty().with_unknown(Kind::bytes())).or_undefined(),
                )
                .with_metadata_field(
                    &owned_value_path!(SimpleHttpConfig::NAME, "query_parameters"),
                    Kind::object(Collection::empty().with_unknown(Kind::bytes())).or_undefined(),
                )
                .with_metadata_field(
                    &owned_value_path!("vector", "ingest_timestamp"),
                    Kind::timestamp(),
                );

        assert_eq!(definition, expected_definition)
    }

    #[test]
    fn output_schema_definition_legacy_namespace() {
        let config = SimpleHttpConfig::default();

        let definition = config.outputs(LogNamespace::Legacy)[0]
            .clone()
            .log_schema_definition
            .unwrap();

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
        .unknown_fields(Kind::bytes());

        assert_eq!(definition, expected_definition)
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
}
