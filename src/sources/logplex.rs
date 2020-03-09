use crate::{
    event::{self, Event},
    tls::{MaybeTlsIncoming, TlsConfig, TlsSettings},
    topology::config::{DataType, GlobalOptions, SourceConfig},
};
use bytes::Buf;
use chrono::{DateTime, Utc};
use futures01::{sync::mpsc, Future, Sink};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use std::{
    io::{BufRead, BufReader},
    net::SocketAddr,
};
use stream_cancel::Tripwire;
use warp::Filter;

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct LogplexConfig {
    address: SocketAddr,
    tls: Option<TlsConfig>,
}

#[typetag::serde(name = "logplex")]
impl SourceConfig for LogplexConfig {
    fn build(
        &self,
        _: &str,
        _: &GlobalOptions,
        out: mpsc::Sender<Event>,
    ) -> crate::Result<super::Source> {
        let (trigger, tripwire) = Tripwire::new();
        let trigger = Arc::new(Mutex::new(Some(trigger)));

        let svc = warp::post2()
            .and(warp::path("events"))
            .and(warp::header::<usize>("Logplex-Msg-Count"))
            .and(warp::header::<String>("Logplex-Frame-Id"))
            .and(warp::header::<String>("Logplex-Drain-Token"))
            .and(warp::body::concat())
            .and_then(move |msg_count, frame_id, drain_token, body| {
                info!(message = "Handling logplex request", %msg_count, %frame_id, %drain_token);

                let events = body_to_events(body);

                if events.len() != msg_count {
                    if cfg!(test) {
                        panic!("Parsed event count does not match message count header");
                    } else {
                        error!(message = "Parsed event count does not match message count header", event_count = events.len(), %msg_count);
                    }
                }

                let out = out.clone();
                let trigger = trigger.clone();
                out.send_all(futures01::stream::iter_ok(events))
                    .map_err(move |_: mpsc::SendError<Event>| {
                        error!("Failed to forward events, downstream is closed");
                        // shut down the http server if someone hasn't already
                        trigger.try_lock().ok().take().map(drop);
                        warp::reject::custom("shutting down")
                })
                .map(|_| warp::reply())
            });

        let ping = warp::get2().and(warp::path("ping")).map(|| "pong");

        let routes = svc.or(ping);

        info!(message = "building logplex server", addr = %self.address);

        let tls = TlsSettings::from_config(&self.tls, true)?;
        let incoming = MaybeTlsIncoming::bind(&self.address, tls)?;

        let server = warp::serve(routes).serve_incoming_with_graceful_shutdown(incoming, tripwire);

        Ok(Box::new(server))
    }

    fn output_type(&self) -> DataType {
        DataType::Log
    }

    fn source_type(&self) -> &'static str {
        "logplex"
    }
}

fn body_to_events(body: impl Buf) -> Vec<Event> {
    let rdr = BufReader::new(body.reader());
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

    if parts.len() == 8 {
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
    }
}

#[cfg(test)]
mod tests {
    use super::LogplexConfig;
    use crate::{
        event::{self, Event},
        runtime::Runtime,
        test_util::{self, collect_n},
        topology::config::{GlobalOptions, SourceConfig},
    };
    use chrono::{DateTime, Utc};
    use futures01::sync::mpsc;
    use http::Method;
    use pretty_assertions::assert_eq;
    use std::net::SocketAddr;

    fn source(rt: &mut Runtime) -> (mpsc::Receiver<Event>, SocketAddr) {
        test_util::trace_init();
        let (sender, recv) = mpsc::channel(100);
        let address = test_util::next_addr();
        rt.spawn(
            LogplexConfig { address, tls: None }
                .build("default", &GlobalOptions::default(), sender)
                .unwrap(),
        );
        (recv, address)
    }

    fn send(address: SocketAddr, body: &str) -> u16 {
        let len = body.lines().count();
        reqwest::Client::new()
            .request(Method::POST, &format!("http://{}/events", address))
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

        let mut rt = test_util::runtime();
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
    }
}
