use std::{
    collections::HashMap,
    io::{BufRead, BufReader},
    net::SocketAddr,
    str::FromStr,
};

use bytes::{Buf, BufMut, Bytes, BytesMut};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;
use tokio_util::codec::Decoder;
use warp::http::{HeaderMap, StatusCode};

use crate::{
    codecs::{
        self,
        decoding::{DecodingConfig, DeserializerConfig, FramingConfig},
    },
    config::{
        log_schema, AcknowledgementsConfig, DataType, GenerateConfig, Output, Resource,
        SourceConfig, SourceContext, SourceDescription,
    },
    event::Event,
    internal_events::{HerokuLogplexRequestReadError, HerokuLogplexRequestReceived},
    serde::{bool_or_struct, default_decoding, default_framing_message_based},
    sources::util::{
        add_query_parameters, ErrorMessage, HttpSource, HttpSourceAuthConfig, StreamDecodingError,
    },
    tls::TlsConfig,
};

#[derive(Deserialize, Serialize, Debug, Clone)]
pub(crate) struct LogplexConfig {
    address: SocketAddr,
    #[serde(default)]
    query_parameters: Vec<String>,
    tls: Option<TlsConfig>,
    auth: Option<HttpSourceAuthConfig>,
    #[serde(default = "default_framing_message_based")]
    framing: FramingConfig,
    #[serde(default = "default_decoding")]
    decoding: DeserializerConfig,
    #[serde(default, deserialize_with = "bool_or_struct")]
    acknowledgements: AcknowledgementsConfig,
}

inventory::submit! {
    SourceDescription::new::<LogplexConfig>("logplex")
}

inventory::submit! {
    SourceDescription::new::<LogplexConfig>("heroku_logs")
}

impl GenerateConfig for LogplexConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(Self {
            address: "0.0.0.0:80".parse().unwrap(),
            query_parameters: Vec::new(),
            tls: None,
            auth: None,
            framing: default_framing_message_based(),
            decoding: default_decoding(),
            acknowledgements: AcknowledgementsConfig::default(),
        })
        .unwrap()
    }
}

#[derive(Clone, Debug, Default)]
struct LogplexSource {
    query_parameters: Vec<String>,
    decoder: codecs::Decoder,
}

impl HttpSource for LogplexSource {
    fn build_events(
        &self,
        body: Bytes,
        header_map: HeaderMap,
        query_parameters: HashMap<String, String>,
        _full_path: &str,
    ) -> Result<Vec<Event>, ErrorMessage> {
        let mut events = decode_message(self.decoder.clone(), body, header_map)?;
        add_query_parameters(&mut events, &self.query_parameters, query_parameters);
        Ok(events)
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "heroku_logs")]
impl SourceConfig for LogplexConfig {
    async fn build(&self, cx: SourceContext) -> crate::Result<super::Source> {
        let decoder = DecodingConfig::new(self.framing.clone(), self.decoding.clone()).build();
        let source = LogplexSource {
            query_parameters: self.query_parameters.clone(),
            decoder,
        };
        source.run(
            self.address,
            "events",
            true,
            &self.tls,
            &self.auth,
            cx,
            self.acknowledgements,
        )
    }

    fn outputs(&self) -> Vec<Output> {
        vec![Output::default(DataType::Log)]
    }

    fn source_type(&self) -> &'static str {
        "heroku_logs"
    }

    fn resources(&self) -> Vec<Resource> {
        vec![Resource::tcp(self.address)]
    }

    fn can_acknowledge(&self) -> bool {
        true
    }
}

// Add a compatibility alias to avoid breaking existing configs
#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct LogplexCompatConfig(LogplexConfig);

#[async_trait::async_trait]
#[typetag::serde(name = "logplex")]
impl SourceConfig for LogplexCompatConfig {
    async fn build(&self, cx: SourceContext) -> crate::Result<super::Source> {
        self.0.build(cx).await
    }

    fn outputs(&self) -> Vec<Output> {
        self.0.outputs()
    }

    fn source_type(&self) -> &'static str {
        self.0.source_type()
    }

    fn resources(&self) -> Vec<Resource> {
        self.0.resources()
    }

    fn can_acknowledge(&self) -> bool {
        true
    }
}

fn decode_message(
    decoder: codecs::Decoder,
    body: Bytes,
    header_map: HeaderMap,
) -> Result<Vec<Event>, ErrorMessage> {
    // Deal with headers
    let msg_count = match usize::from_str(get_header(&header_map, "Logplex-Msg-Count")?) {
        Ok(v) => v,
        Err(e) => return Err(header_error_message("Logplex-Msg-Count", &e.to_string())),
    };
    let frame_id = get_header(&header_map, "Logplex-Frame-Id")?;
    let drain_token = get_header(&header_map, "Logplex-Drain-Token")?;

    emit!(&HerokuLogplexRequestReceived {
        msg_count,
        frame_id,
        drain_token
    });

    // Deal with body
    let events = body_to_events(decoder, body);

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

fn body_to_events(decoder: codecs::Decoder, body: Bytes) -> Vec<Event> {
    let rdr = BufReader::new(body.reader());
    rdr.lines()
        .filter_map(|res| {
            res.map_err(|error| emit!(&HerokuLogplexRequestReadError { error }))
                .ok()
        })
        .filter(|s| !s.is_empty())
        .flat_map(|line| line_to_events(decoder.clone(), line))
        .collect()
}

fn line_to_events(mut decoder: codecs::Decoder, line: String) -> SmallVec<[Event; 1]> {
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

        loop {
            match decoder.decode_eof(&mut buffer) {
                Ok(Some((decoded, _byte_size))) => {
                    for mut event in decoded {
                        if let Event::Log(ref mut log) = event {
                            if let Ok(ts) = timestamp.parse::<DateTime<Utc>>() {
                                log.try_insert(log_schema().timestamp_key(), ts);
                            }

                            log.try_insert(log_schema().host_key(), hostname.to_owned());

                            log.try_insert_flat("app_name", app_name.to_owned());
                            log.try_insert_flat("proc_id", proc_id.to_owned());
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
            internal_log_rate_secs = 10
        );

        events.push(Event::from(line))
    };

    let now = Utc::now();

    for event in &mut events {
        if let Event::Log(log) = event {
            log.try_insert(log_schema().source_type_key(), Bytes::from("heroku_logs"));
            log.try_insert(log_schema().timestamp_key(), now);
        }
    }

    events
}

#[cfg(test)]
mod tests {
    use std::net::SocketAddr;

    use chrono::{DateTime, Utc};
    use futures::Stream;
    use pretty_assertions::assert_eq;
    use vector_core::event::{Event, EventStatus, Value};

    use super::{HttpSourceAuthConfig, LogplexConfig};
    use crate::{
        config::{log_schema, SourceConfig, SourceContext},
        serde::{default_decoding, default_framing_message_based},
        test_util::{components, next_addr, random_string, spawn_collect_n, wait_for_tcp},
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
    ) -> (impl Stream<Item = Event>, SocketAddr) {
        components::init_test();
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
        let mut req = reqwest::Client::new().post(&format!("http://{}/events?{}", address, query));
        if let Some(auth) = auth {
            req = req.basic_auth(auth.username, Some(auth.password));
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
            password: random_string(16),
        }
    }

    const SAMPLE_BODY: &str = r#"267 <158>1 2020-01-08T22:33:57.353034+00:00 host heroku router - at=info method=GET path="/cart_link" host=lumberjack-store.timber.io request_id=05726858-c44e-4f94-9a20-37df73be9006 fwd="73.75.38.87" dyno=web.1 connect=1ms service=22ms status=304 bytes=656 protocol=http"#;

    #[tokio::test]
    async fn logplex_handles_router_log() {
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
        components::SOURCE_TESTS.assert(&["http_path"]);

        let event = events.remove(0);
        let log = event.as_log();

        assert_eq!(
            log[log_schema().message_key()],
            r#"at=info method=GET path="/cart_link" host=lumberjack-store.timber.io request_id=05726858-c44e-4f94-9a20-37df73be9006 fwd="73.75.38.87" dyno=web.1 connect=1ms service=22ms status=304 bytes=656 protocol=http"#.into()
        );
        assert_eq!(
            log[log_schema().timestamp_key()],
            "2020-01-08T22:33:57.353034+00:00"
                .parse::<DateTime<Utc>>()
                .unwrap()
                .into()
        );
        assert_eq!(log[&log_schema().host_key()], "host".into());
        assert_eq!(log[log_schema().source_type_key()], "heroku_logs".into());
        assert_eq!(log["appname"], "lumberjack-store".into());
        assert_eq!(log["absent"], Value::Null);
    }

    #[tokio::test]
    async fn logplex_handles_failures() {
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
        components::SOURCE_TESTS.assert(&["http_path"]);

        assert_eq!(events.len(), SAMPLE_BODY.lines().count());
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
        let body = "267 <158>1 2020-01-08T22:33:57.353034+00:00 host heroku router - foo bar baz";
        let events = super::line_to_events(Default::default(), body.into());
        let log = events[0].as_log();

        assert_eq!(log[log_schema().message_key()], "foo bar baz".into());
        assert_eq!(
            log[log_schema().timestamp_key()],
            "2020-01-08T22:33:57.353034+00:00"
                .parse::<DateTime<Utc>>()
                .unwrap()
                .into()
        );
        assert_eq!(log[log_schema().host_key()], "host".into());
        assert_eq!(log[log_schema().source_type_key()], "heroku_logs".into());
    }

    #[test]
    fn logplex_handles_malformed_lines() {
        let body = "what am i doing here";
        let events = super::line_to_events(Default::default(), body.into());
        let log = events[0].as_log();

        assert_eq!(
            log[log_schema().message_key()],
            "what am i doing here".into()
        );
        assert!(log.get(log_schema().timestamp_key()).is_some());
        assert_eq!(log[log_schema().source_type_key()], "heroku_logs".into());
    }

    #[test]
    fn logplex_doesnt_blow_up_on_bad_framing() {
        let body = "1000000 <158>1 2020-01-08T22:33:57.353034+00:00 host heroku router - i'm not that long";
        let events = super::line_to_events(Default::default(), body.into());
        let log = events[0].as_log();

        assert_eq!(log[log_schema().message_key()], "i'm not that long".into());
        assert_eq!(
            log[log_schema().timestamp_key()],
            "2020-01-08T22:33:57.353034+00:00"
                .parse::<DateTime<Utc>>()
                .unwrap()
                .into()
        );
        assert_eq!(log[log_schema().host_key()], "host".into());
        assert_eq!(log[log_schema().source_type_key()], "heroku_logs".into());
    }
}
