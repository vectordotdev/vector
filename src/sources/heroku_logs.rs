use std::{
    collections::HashMap,
    io::{BufRead, BufReader},
    net::SocketAddr,
    str::FromStr,
};

use bytes::{Buf, BufMut, Bytes, BytesMut};
use chrono::{DateTime, Utc};
use smallvec::SmallVec;
use tokio_util::codec::Decoder as _;
use vector_lib::codecs::{
    decoding::{DeserializerConfig, FramingConfig},
    StreamDecodingError,
};
use vector_lib::lookup::{lookup_v2::parse_value_path, owned_value_path, path};
use vrl::value::{kind::Collection, Kind};
use warp::http::{HeaderMap, StatusCode};

use vector_lib::configurable::configurable_component;
use vector_lib::{
    config::{LegacyKey, LogNamespace},
    schema::Definition,
};

use crate::{
    codecs::{Decoder, DecodingConfig},
    config::{
        log_schema, GenerateConfig, Resource, SourceAcknowledgementsConfig, SourceConfig,
        SourceContext, SourceOutput,
    },
    event::{Event, LogEvent},
    http::KeepaliveConfig,
    internal_events::{HerokuLogplexRequestReadError, HerokuLogplexRequestReceived},
    serde::{bool_or_struct, default_decoding, default_framing_message_based},
    sources::util::{
        http::{add_query_parameters, HttpMethod},
        ErrorMessage, HttpSource, HttpSourceAuthConfig,
    },
    tls::TlsEnableableConfig,
};

/// Configuration for `heroku_logs` source.
#[configurable_component(source("heroku_logs", "Collect logs from Heroku's Logplex, the router responsible for receiving logs from your Heroku apps."))]
#[derive(Clone, Debug)]
pub struct LogplexConfig {
    /// The socket address to listen for connections on.
    #[configurable(metadata(docs::examples = "0.0.0.0:80"))]
    #[configurable(metadata(docs::examples = "localhost:80"))]
    address: SocketAddr,

    /// A list of URL query parameters to include in the log event.
    ///
    /// These override any values included in the body with conflicting names.
    #[serde(default)]
    #[configurable(metadata(docs::examples = "application", docs::examples = "source"))]
    query_parameters: Vec<String>,

    #[configurable(derived)]
    tls: Option<TlsEnableableConfig>,

    #[configurable(derived)]
    auth: Option<HttpSourceAuthConfig>,

    #[configurable(derived)]
    #[serde(default = "default_framing_message_based")]
    framing: FramingConfig,

    #[configurable(derived)]
    #[serde(default = "default_decoding")]
    decoding: DeserializerConfig,

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

impl LogplexConfig {
    /// Builds the `schema::Definition` for this source using the provided `LogNamespace`.
    fn schema_definition(&self, log_namespace: LogNamespace) -> Definition {
        let mut schema_definition = self
            .decoding
            .schema_definition(log_namespace)
            .with_standard_vector_source_metadata()
            .with_source_metadata(
                LogplexConfig::NAME,
                None,
                &owned_value_path!("timestamp"),
                Kind::timestamp().or_undefined(),
                Some("timestamp"),
            )
            .with_source_metadata(
                LogplexConfig::NAME,
                log_schema()
                    .host_key()
                    .cloned()
                    .map(LegacyKey::InsertIfEmpty),
                &owned_value_path!("host"),
                Kind::bytes(),
                Some("host"),
            )
            .with_source_metadata(
                LogplexConfig::NAME,
                Some(LegacyKey::InsertIfEmpty(owned_value_path!("app_name"))),
                &owned_value_path!("app_name"),
                Kind::bytes(),
                Some("service"),
            )
            .with_source_metadata(
                LogplexConfig::NAME,
                Some(LegacyKey::InsertIfEmpty(owned_value_path!("proc_id"))),
                &owned_value_path!("proc_id"),
                Kind::bytes(),
                None,
            )
            // for metadata that is added to the events dynamically from the self.query_parameters
            .with_source_metadata(
                LogplexConfig::NAME,
                None,
                &owned_value_path!("query_parameters"),
                Kind::object(Collection::empty().with_unknown(Kind::bytes())).or_undefined(),
                None,
            );

        // for metadata that is added to the events dynamically from config options
        if log_namespace == LogNamespace::Legacy {
            schema_definition = schema_definition.unknown_fields(Kind::bytes());
        }

        schema_definition
    }
}

impl Default for LogplexConfig {
    fn default() -> Self {
        Self {
            address: "0.0.0.0:80".parse().unwrap(),
            query_parameters: Vec::new(),
            tls: None,
            auth: None,
            framing: default_framing_message_based(),
            decoding: default_decoding(),
            acknowledgements: SourceAcknowledgementsConfig::default(),
            log_namespace: None,
            keepalive: KeepaliveConfig::default(),
        }
    }
}

impl GenerateConfig for LogplexConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(LogplexConfig::default()).unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "heroku_logs")]
impl SourceConfig for LogplexConfig {
    async fn build(&self, cx: SourceContext) -> crate::Result<super::Source> {
        let log_namespace = cx.log_namespace(self.log_namespace);

        let decoder =
            DecodingConfig::new(self.framing.clone(), self.decoding.clone(), log_namespace)
                .build()?;

        let source = LogplexSource {
            query_parameters: self.query_parameters.clone(),
            decoder,
            log_namespace,
        };

        source.run(
            self.address,
            "events",
            HttpMethod::Post,
            StatusCode::OK,
            true,
            &self.tls,
            &self.auth,
            cx,
            self.acknowledgements,
            self.keepalive.clone(),
        )
    }

    fn outputs(&self, global_log_namespace: LogNamespace) -> Vec<SourceOutput> {
        // There is a global and per-source `log_namespace` config.
        // The source config overrides the global setting and is merged here.
        let schema_def = self.schema_definition(global_log_namespace.merge(self.log_namespace));
        vec![SourceOutput::new_maybe_logs(
            self.decoding.output_type(),
            schema_def,
        )]
    }

    fn resources(&self) -> Vec<Resource> {
        vec![Resource::tcp(self.address)]
    }

    fn can_acknowledge(&self) -> bool {
        true
    }
}

#[derive(Clone, Default)]
struct LogplexSource {
    query_parameters: Vec<String>,
    decoder: Decoder,
    log_namespace: LogNamespace,
}

impl LogplexSource {
    fn decode_message(
        &self,
        body: Bytes,
        header_map: &HeaderMap,
    ) -> Result<Vec<Event>, ErrorMessage> {
        // Deal with headers
        let msg_count = match usize::from_str(get_header(header_map, "Logplex-Msg-Count")?) {
            Ok(v) => v,
            Err(e) => return Err(header_error_message("Logplex-Msg-Count", &e.to_string())),
        };
        let frame_id = get_header(header_map, "Logplex-Frame-Id")?;
        let drain_token = get_header(header_map, "Logplex-Drain-Token")?;

        emit!(HerokuLogplexRequestReceived {
            msg_count,
            frame_id,
            drain_token
        });

        // Deal with body
        let events = self.body_to_events(body);

        if events.len() != msg_count {
            let error_msg = format!(
                "Parsed event count does not match message count header: {} vs {}",
                events.len(),
                msg_count
            );

            if cfg!(test) {
                panic!("{}", error_msg);
            }
            return Err(header_error_message("Logplex-Msg-Count", &error_msg));
        }

        Ok(events)
    }

    fn body_to_events(&self, body: Bytes) -> Vec<Event> {
        let rdr = BufReader::new(body.reader());
        rdr.lines()
            .filter_map(|res| {
                res.map_err(|error| emit!(HerokuLogplexRequestReadError { error }))
                    .ok()
            })
            .filter(|s| !s.is_empty())
            .flat_map(|line| line_to_events(self.decoder.clone(), self.log_namespace, line))
            .collect()
    }
}

impl HttpSource for LogplexSource {
    fn build_events(
        &self,
        body: Bytes,
        header_map: &HeaderMap,
        _query_parameters: &HashMap<String, String>,
        _full_path: &str,
    ) -> Result<Vec<Event>, ErrorMessage> {
        self.decode_message(body, header_map)
    }

    fn enrich_events(
        &self,
        events: &mut [Event],
        _request_path: &str,
        _headers_config: &HeaderMap,
        query_parameters: &HashMap<String, String>,
        _source_ip: Option<&SocketAddr>,
    ) {
        add_query_parameters(
            events,
            &self.query_parameters,
            query_parameters,
            self.log_namespace,
            LogplexConfig::NAME,
        );
    }
}

fn get_header<'a>(header_map: &'a HeaderMap, name: &str) -> Result<&'a str, ErrorMessage> {
    if let Some(header_value) = header_map.get(name) {
        header_value
            .to_str()
            .map_err(|e| header_error_message(name, &e.to_string()))
    } else {
        Err(header_error_message(name, "Header does not exist"))
    }
}

fn header_error_message(name: &str, msg: &str) -> ErrorMessage {
    ErrorMessage::new(
        StatusCode::BAD_REQUEST,
        format!("Invalid request header {:?}: {:?}", name, msg),
    )
}

fn line_to_events(
    mut decoder: Decoder,
    log_namespace: LogNamespace,
    line: String,
) -> SmallVec<[Event; 1]> {
    let parts = line.splitn(8, ' ').collect::<Vec<&str>>();

    let mut events = SmallVec::<[Event; 1]>::new();

    if parts.len() == 8 {
        let timestamp = parts[2];
        let hostname = parts[3];
        let app_name = parts[4];
        let proc_id = parts[5];
        let message = parts[7];

        let mut buffer = BytesMut::new();
        buffer.put(message.as_bytes());

        let legacy_host_key = log_schema().host_key().cloned();
        let legacy_app_key = parse_value_path("app_name").ok();
        let legacy_proc_key = parse_value_path("proc_id").ok();

        loop {
            match decoder.decode_eof(&mut buffer) {
                Ok(Some((decoded, _byte_size))) => {
                    for mut event in decoded {
                        if let Event::Log(ref mut log) = event {
                            if let Ok(ts) = timestamp.parse::<DateTime<Utc>>() {
                                log_namespace.insert_vector_metadata(
                                    log,
                                    log_schema().timestamp_key(),
                                    path!("timestamp"),
                                    ts,
                                );
                            }

                            log_namespace.insert_source_metadata(
                                LogplexConfig::NAME,
                                log,
                                legacy_host_key.as_ref().map(LegacyKey::InsertIfEmpty),
                                path!("host"),
                                hostname.to_owned(),
                            );

                            log_namespace.insert_source_metadata(
                                LogplexConfig::NAME,
                                log,
                                legacy_app_key.as_ref().map(LegacyKey::InsertIfEmpty),
                                path!("app_name"),
                                app_name.to_owned(),
                            );

                            log_namespace.insert_source_metadata(
                                LogplexConfig::NAME,
                                log,
                                legacy_proc_key.as_ref().map(LegacyKey::InsertIfEmpty),
                                path!("proc_id"),
                                proc_id.to_owned(),
                            );
                        }

                        events.push(event);
                    }
                }
                Ok(None) => break,
                Err(error) => {
                    if !error.can_continue() {
                        break;
                    }
                }
            }
        }
    } else {
        warn!(
            message = "Line didn't match expected logplex format, so raw message is forwarded.",
            fields = parts.len(),
            internal_log_rate_limit = true
        );

        events.push(LogEvent::from_str_legacy(line).into())
    };

    let now = Utc::now();

    for event in &mut events {
        if let Event::Log(log) = event {
            log_namespace.insert_standard_vector_source_metadata(log, LogplexConfig::NAME, now);
        }
    }

    events
}

#[cfg(test)]
mod tests {
    use std::net::SocketAddr;

    use chrono::{DateTime, Utc};
    use futures::Stream;
    use similar_asserts::assert_eq;
    use vector_lib::lookup::{owned_value_path, OwnedTargetPath};
    use vector_lib::{
        config::LogNamespace,
        event::{Event, EventStatus, Value},
        schema::Definition,
    };
    use vrl::value::{kind::Collection, Kind};

    use super::{HttpSourceAuthConfig, LogplexConfig};
    use crate::{
        config::{log_schema, SourceConfig, SourceContext},
        serde::{default_decoding, default_framing_message_based},
        test_util::{
            components::{assert_source_compliance, HTTP_PUSH_SOURCE_TAGS},
            next_addr, random_string, spawn_collect_n, wait_for_tcp,
        },
        SourceSender,
    };

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<LogplexConfig>();
    }

    async fn source(
        auth: Option<HttpSourceAuthConfig>,
        query_parameters: Vec<String>,
        status: EventStatus,
        acknowledgements: bool,
    ) -> (impl Stream<Item = Event> + Unpin, SocketAddr) {
        let (sender, recv) = SourceSender::new_test_finalize(status);
        let address = next_addr();
        let context = SourceContext::new_test(sender, None);
        tokio::spawn(async move {
            LogplexConfig {
                address,
                query_parameters,
                tls: None,
                auth,
                framing: default_framing_message_based(),
                decoding: default_decoding(),
                acknowledgements: acknowledgements.into(),
                log_namespace: None,
                keepalive: Default::default(),
            }
            .build(context)
            .await
            .unwrap()
            .await
            .unwrap()
        });
        wait_for_tcp(address).await;
        (recv, address)
    }

    async fn send(
        address: SocketAddr,
        body: &str,
        auth: Option<HttpSourceAuthConfig>,
        query: &str,
    ) -> u16 {
        let len = body.lines().count();
        let mut req = reqwest::Client::new().post(format!("http://{}/events?{}", address, query));
        if let Some(auth) = auth {
            req = req.basic_auth(auth.username, Some(auth.password.inner()));
        }
        req.header("Logplex-Msg-Count", len)
            .header("Logplex-Frame-Id", "frame-foo")
            .header("Logplex-Drain-Token", "drain-bar")
            .body(body.to_owned())
            .send()
            .await
            .unwrap()
            .status()
            .as_u16()
    }

    fn make_auth() -> HttpSourceAuthConfig {
        HttpSourceAuthConfig {
            username: random_string(16),
            password: random_string(16).into(),
        }
    }

    const SAMPLE_BODY: &str = r#"267 <158>1 2020-01-08T22:33:57.353034+00:00 host heroku router - at=info method=GET path="/cart_link" host=lumberjack-store.timber.io request_id=05726858-c44e-4f94-9a20-37df73be9006 fwd="73.75.38.87" dyno=web.1 connect=1ms service=22ms status=304 bytes=656 protocol=http"#;

    #[tokio::test]
    async fn logplex_handles_router_log() {
        assert_source_compliance(&HTTP_PUSH_SOURCE_TAGS, async {
            let auth = make_auth();

            let (rx, addr) = source(
                Some(auth.clone()),
                vec!["appname".to_string(), "absent".to_string()],
                EventStatus::Delivered,
                true,
            )
            .await;

            let mut events = spawn_collect_n(
                async move {
                    assert_eq!(
                        200,
                        send(addr, SAMPLE_BODY, Some(auth), "appname=lumberjack-store").await
                    )
                },
                rx,
                SAMPLE_BODY.lines().count(),
            )
            .await;

            let event = events.remove(0);
            let log = event.as_log();

            assert_eq!(
                *log.get_message().unwrap(),
                r#"at=info method=GET path="/cart_link" host=lumberjack-store.timber.io request_id=05726858-c44e-4f94-9a20-37df73be9006 fwd="73.75.38.87" dyno=web.1 connect=1ms service=22ms status=304 bytes=656 protocol=http"#.into()
            );
            assert_eq!(
                log[log_schema().timestamp_key().unwrap().to_string()],
                "2020-01-08T22:33:57.353034+00:00"
                    .parse::<DateTime<Utc>>()
                    .unwrap()
                    .into()
            );
            assert_eq!(*log.get_host().unwrap(), "host".into());
            assert_eq!(*log.get_source_type().unwrap(), "heroku_logs".into());
            assert_eq!(log["appname"], "lumberjack-store".into());
            assert_eq!(log["absent"], Value::Null);
        }).await;
    }

    #[tokio::test]
    async fn logplex_handles_failures() {
        assert_source_compliance(&HTTP_PUSH_SOURCE_TAGS, async {
            let auth = make_auth();

            let (rx, addr) = source(Some(auth.clone()), vec![], EventStatus::Rejected, true).await;

            let events = spawn_collect_n(
                async move {
                    assert_eq!(
                        400,
                        send(addr, SAMPLE_BODY, Some(auth), "appname=lumberjack-store").await
                    )
                },
                rx,
                SAMPLE_BODY.lines().count(),
            )
            .await;

            assert_eq!(events.len(), SAMPLE_BODY.lines().count());
        })
        .await;
    }

    #[tokio::test]
    async fn logplex_ignores_disabled_acknowledgements() {
        let auth = make_auth();

        let (rx, addr) = source(Some(auth.clone()), vec![], EventStatus::Rejected, false).await;

        let events = spawn_collect_n(
            async move {
                assert_eq!(
                    200,
                    send(addr, SAMPLE_BODY, Some(auth), "appname=lumberjack-store").await
                )
            },
            rx,
            SAMPLE_BODY.lines().count(),
        )
        .await;

        assert_eq!(events.len(), SAMPLE_BODY.lines().count());
    }

    #[tokio::test]
    async fn logplex_auth_failure() {
        let (_rx, addr) = source(Some(make_auth()), vec![], EventStatus::Delivered, true).await;

        assert_eq!(
            401,
            send(
                addr,
                SAMPLE_BODY,
                Some(make_auth()),
                "appname=lumberjack-store"
            )
            .await
        );
    }

    #[test]
    fn logplex_handles_normal_lines() {
        let log_namespace = LogNamespace::Legacy;
        let body = "267 <158>1 2020-01-08T22:33:57.353034+00:00 host heroku router - foo bar baz";
        let events = super::line_to_events(Default::default(), log_namespace, body.into());
        let log = events[0].as_log();

        assert_eq!(*log.get_message().unwrap(), "foo bar baz".into());
        assert_eq!(
            log[log_schema().timestamp_key().unwrap().to_string()],
            "2020-01-08T22:33:57.353034+00:00"
                .parse::<DateTime<Utc>>()
                .unwrap()
                .into()
        );
        assert_eq!(*log.get_host().unwrap(), "host".into());
        assert_eq!(*log.get_source_type().unwrap(), "heroku_logs".into());
    }

    #[test]
    fn logplex_handles_malformed_lines() {
        let log_namespace = LogNamespace::Legacy;
        let body = "what am i doing here";
        let events = super::line_to_events(Default::default(), log_namespace, body.into());
        let log = events[0].as_log();

        assert_eq!(*log.get_message().unwrap(), "what am i doing here".into());
        assert!(log.get_timestamp().is_some());
        assert_eq!(*log.get_source_type().unwrap(), "heroku_logs".into());
    }

    #[test]
    fn logplex_doesnt_blow_up_on_bad_framing() {
        let log_namespace = LogNamespace::Legacy;
        let body = "1000000 <158>1 2020-01-08T22:33:57.353034+00:00 host heroku router - i'm not that long";
        let events = super::line_to_events(Default::default(), log_namespace, body.into());
        let log = events[0].as_log();

        assert_eq!(*log.get_message().unwrap(), "i'm not that long".into());
        assert_eq!(
            log[log_schema().timestamp_key().unwrap().to_string()],
            "2020-01-08T22:33:57.353034+00:00"
                .parse::<DateTime<Utc>>()
                .unwrap()
                .into()
        );
        assert_eq!(*log.get_host().unwrap(), "host".into());
        assert_eq!(*log.get_source_type().unwrap(), "heroku_logs".into());
    }

    #[test]
    fn output_schema_definition_vector_namespace() {
        let config = LogplexConfig {
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
                    &owned_value_path!("vector", "ingest_timestamp"),
                    Kind::timestamp(),
                    None,
                )
                .with_metadata_field(
                    &owned_value_path!(LogplexConfig::NAME, "timestamp"),
                    Kind::timestamp().or_undefined(),
                    Some("timestamp"),
                )
                .with_metadata_field(
                    &owned_value_path!(LogplexConfig::NAME, "host"),
                    Kind::bytes(),
                    Some("host"),
                )
                .with_metadata_field(
                    &owned_value_path!(LogplexConfig::NAME, "app_name"),
                    Kind::bytes(),
                    Some("service"),
                )
                .with_metadata_field(
                    &owned_value_path!(LogplexConfig::NAME, "proc_id"),
                    Kind::bytes(),
                    None,
                )
                .with_metadata_field(
                    &owned_value_path!(LogplexConfig::NAME, "query_parameters"),
                    Kind::object(Collection::empty().with_unknown(Kind::bytes())).or_undefined(),
                    None,
                );

        assert_eq!(definitions, Some(expected_definition))
    }

    #[test]
    fn output_schema_definition_legacy_namespace() {
        let config = LogplexConfig::default();

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
        .with_event_field(&owned_value_path!("host"), Kind::bytes(), Some("host"))
        .with_event_field(
            &owned_value_path!("app_name"),
            Kind::bytes(),
            Some("service"),
        )
        .with_event_field(&owned_value_path!("proc_id"), Kind::bytes(), None)
        .unknown_fields(Kind::bytes());

        assert_eq!(definitions, Some(expected_definition))
    }
}
