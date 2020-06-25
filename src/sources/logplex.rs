use crate::{
    event::{self, Event},
    shutdown::ShutdownSignal,
    sources::util::{ErrorMessage, HttpSource},
    tls::TlsConfig,
    topology::config::{DataType, GlobalOptions, SourceConfig},
};
use bytes05::Bytes;
use chrono::{DateTime, Utc};
use futures01::sync::mpsc;
use serde::{Deserialize, Serialize};
use std::{
    io::{BufRead, BufReader, Cursor},
    net::SocketAddr,
    str::FromStr,
};
use warp::http::{HeaderMap, StatusCode};

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct LogplexConfig {
    address: SocketAddr,
    tls: Option<TlsConfig>,
}

#[derive(Clone, Default)]
struct LogplexSource {}

impl HttpSource for LogplexSource {
    fn build_event(&self, body: Bytes, header_map: HeaderMap) -> Result<Vec<Event>, ErrorMessage> {
        decode_message(body, header_map)
    }
}

#[typetag::serde(name = "logplex")]
impl SourceConfig for LogplexConfig {
    fn build(
        &self,
        _: &str,
        _: &GlobalOptions,
        shutdown: ShutdownSignal,
        out: mpsc::Sender<Event>,
    ) -> crate::Result<super::Source> {
        let source = LogplexSource::default();
        source.run(self.address, "events", &self.tls, out, shutdown)
    }

    fn output_type(&self) -> DataType {
        DataType::Log
    }

    fn source_type(&self) -> &'static str {
        "logplex"
    }
}

fn decode_message(body: Bytes, header_map: HeaderMap) -> Result<Vec<Event>, ErrorMessage> {
    // Deal with headers
    let msg_count = match usize::from_str(get_header(&header_map, "Logplex-Msg-Count")?) {
        Ok(v) => v,
        Err(e) => return Err(header_error_message("Logplex-Msg-Count", &e.to_string())),
    };
    let frame_id = get_header(&header_map, "Logplex-Frame-Id")?;
    let drain_token = get_header(&header_map, "Logplex-Drain-Token")?;
    info!(message = "Handling logplex request", %msg_count, %frame_id, %drain_token);

    // Deal with body
    let events = body_to_events(body);

    if events.len() != msg_count {
        let error_msg = format!(
            "Parsed event count does not match message count header: {} vs {}",
            events.len(),
            msg_count
        );

        if cfg!(test) {
            panic!(error_msg);
        } else {
            error!(message = error_msg.as_str());
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

fn body_to_events(body: Bytes) -> Vec<Event> {
    let rdr = BufReader::new(Cursor::new(body));
    rdr.lines()
        .filter_map(|res| {
            res.map_err(|error| error!(message = "Error reading request body", ?error))
                .ok()
        })
        .filter(|s| !s.is_empty())
        .map(line_to_event)
        .collect()
}

fn line_to_event(line: String) -> Event {
    let parts = line.splitn(8, ' ').collect::<Vec<&str>>();

    let mut event = if parts.len() == 8 {
        let timestamp = parts[2];
        let hostname = parts[3];
        let app_name = parts[4];
        let proc_id = parts[5];
        let message = parts[7];

        let mut event = Event::from(message);
        let log = event.as_mut_log();

        if let Ok(ts) = timestamp.parse::<DateTime<Utc>>() {
            log.insert(event::log_schema().timestamp_key().clone(), ts);
        }

        log.insert(event::log_schema().host_key().clone(), hostname);

        log.insert("app_name", app_name);
        log.insert("proc_id", proc_id);

        event
    } else {
        warn!(
            message = "Line didn't match expected logplex format. Forwarding raw message.",
            fields = parts.len()
        );
        Event::from(line)
    };

    // Add source type
    event
        .as_mut_log()
        .try_insert(event::log_schema().source_type_key(), "logplex");

    event
}

#[cfg(test)]
mod tests {
    use super::LogplexConfig;
    use crate::shutdown::ShutdownSignal;
    use crate::{
        event::{self, Event},
        runtime::Runtime,
        test_util::{self, collect_n, runtime},
        topology::config::{GlobalOptions, SourceConfig},
    };
    use chrono::{DateTime, Utc};
    use futures01::sync::mpsc;
    use pretty_assertions::assert_eq;
    use std::net::SocketAddr;

    fn source(rt: &mut Runtime) -> (mpsc::Receiver<Event>, SocketAddr) {
        test_util::trace_init();
        let (sender, recv) = mpsc::channel(100);
        let address = test_util::next_addr();
        rt.spawn(
            LogplexConfig { address, tls: None }
                .build(
                    "default",
                    &GlobalOptions::default(),
                    ShutdownSignal::noop(),
                    sender,
                )
                .unwrap(),
        );
        (recv, address)
    }

    fn send(address: SocketAddr, body: &str) -> u16 {
        let len = body.lines().count();
        reqwest::Client::new()
            .post(&format!("http://{}/events", address))
            .header("Logplex-Msg-Count", len)
            .header("Logplex-Frame-Id", "frame-foo")
            .header("Logplex-Drain-Token", "drain-bar")
            .body(body.to_owned())
            .send()
            .unwrap()
            .status()
            .as_u16()
    }

    #[test]
    fn logplex_handles_router_log() {
        let body = r#"267 <158>1 2020-01-08T22:33:57.353034+00:00 host heroku router - at=info method=GET path="/cart_link" host=lumberjack-store.timber.io request_id=05726858-c44e-4f94-9a20-37df73be9006 fwd="73.75.38.87" dyno=web.1 connect=1ms service=22ms status=304 bytes=656 protocol=http"#;

        let mut rt = runtime();
        let (rx, addr) = source(&mut rt);

        assert_eq!(200, send(addr, body));

        let mut events = rt.block_on(collect_n(rx, body.lines().count())).unwrap();
        let event = events.remove(0);
        let log = event.as_log();

        assert_eq!(
            log[&event::log_schema().message_key()],
            r#"at=info method=GET path="/cart_link" host=lumberjack-store.timber.io request_id=05726858-c44e-4f94-9a20-37df73be9006 fwd="73.75.38.87" dyno=web.1 connect=1ms service=22ms status=304 bytes=656 protocol=http"#.into()
        );
        assert_eq!(
            log[&event::log_schema().timestamp_key()],
            "2020-01-08T22:33:57.353034+00:00"
                .parse::<DateTime<Utc>>()
                .unwrap()
                .into()
        );
        assert_eq!(log[&event::log_schema().host_key()], "host".into());
        assert_eq!(log[event::log_schema().source_type_key()], "logplex".into());
    }

    #[test]
    fn logplex_handles_normal_lines() {
        let body = "267 <158>1 2020-01-08T22:33:57.353034+00:00 host heroku router - foo bar baz";
        let event = super::line_to_event(body.into());
        let log = event.as_log();

        assert_eq!(
            log[&event::log_schema().message_key()],
            "foo bar baz".into()
        );
        assert_eq!(
            log[&event::log_schema().timestamp_key()],
            "2020-01-08T22:33:57.353034+00:00"
                .parse::<DateTime<Utc>>()
                .unwrap()
                .into()
        );
        assert_eq!(log[&event::log_schema().host_key()], "host".into());
        assert_eq!(log[event::log_schema().source_type_key()], "logplex".into());
    }

    #[test]
    fn logplex_handles_malformed_lines() {
        let body = "what am i doing here";
        let event = super::line_to_event(body.into());
        let log = event.as_log();

        assert_eq!(
            log[&event::log_schema().message_key()],
            "what am i doing here".into()
        );
        assert!(log.get(&event::log_schema().timestamp_key()).is_some());
        assert_eq!(log[event::log_schema().source_type_key()], "logplex".into());
    }

    #[test]
    fn logplex_doesnt_blow_up_on_bad_framing() {
        let body = "1000000 <158>1 2020-01-08T22:33:57.353034+00:00 host heroku router - i'm not that long";
        let event = super::line_to_event(body.into());
        let log = event.as_log();

        assert_eq!(
            log[&event::log_schema().message_key()],
            "i'm not that long".into()
        );
        assert_eq!(
            log[&event::log_schema().timestamp_key()],
            "2020-01-08T22:33:57.353034+00:00"
                .parse::<DateTime<Utc>>()
                .unwrap()
                .into()
        );
        assert_eq!(log[&event::log_schema().host_key()], "host".into());
        assert_eq!(log[event::log_schema().source_type_key()], "logplex".into());
    }
}
