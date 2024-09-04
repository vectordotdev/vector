#![deny(warnings)]

#[macro_use]
extern crate tracing;

pub mod controller;
pub mod notification;
pub mod topology;

use std::{borrow::Cow, collections::BTreeMap};

use colored::{ColoredString, Colorize};
use tokio::sync::mpsc as tokio_mpsc;
use tokio::time::timeout;
use tokio::time::{Duration, Instant};
use tokio_stream::StreamExt;
use url::Url;

use vector_api_client::{
    connect_subscription_client,
    gql::{
        output_events_by_component_id_patterns_subscription::OutputEventsByComponentIdPatternsSubscriptionOutputEventsByComponentIdPatterns as GraphQLTapOutputEvent,
        TapEncodingFormat, TapSubscriptionExt,
    },
};

#[derive(Clone, Debug)]
pub struct EventFormatter {
    meta: bool,
    format: TapEncodingFormat,
    component_id_label: ColoredString,
    component_kind_label: ColoredString,
    component_type_label: ColoredString,
}

impl EventFormatter {
    pub fn new(meta: bool, format: TapEncodingFormat) -> Self {
        Self {
            meta,
            format,
            component_id_label: "component_id".green(),
            component_kind_label: "component_kind".green(),
            component_type_label: "component_type".green(),
        }
    }

    pub fn format<'a>(
        &self,
        component_id: &str,
        component_kind: &str,
        component_type: &str,
        event: &'a str,
    ) -> Cow<'a, str> {
        if self.meta {
            match self.format {
                TapEncodingFormat::Json => format!(
                    r#"{{"{}":"{}","{}":"{}","{}":"{}","event":{}}}"#,
                    self.component_id_label,
                    component_id.green(),
                    self.component_kind_label,
                    component_kind.green(),
                    self.component_type_label,
                    component_type.green(),
                    event
                )
                .into(),
                TapEncodingFormat::Yaml => {
                    let mut value: BTreeMap<String, serde_yaml::Value> = BTreeMap::new();
                    value.insert("event".to_string(), serde_yaml::from_str(event).unwrap());
                    // We interpolate to include component_id rather than
                    // include it in the map to correctly preserve color
                    // formatting
                    format!(
                        "{}{}: {}\n{}: {}\n{}: {}\n",
                        serde_yaml::to_string(&value).unwrap(),
                        self.component_id_label,
                        component_id.green(),
                        self.component_kind_label,
                        component_kind.green(),
                        self.component_type_label,
                        component_type.green()
                    )
                    .into()
                }
                TapEncodingFormat::Logfmt => format!(
                    "{}={} {}={} {}={} {}",
                    self.component_id_label,
                    component_id.green(),
                    self.component_kind_label,
                    component_kind.green(),
                    self.component_type_label,
                    component_type.green(),
                    event
                )
                .into(),
            }
        } else {
            event.into()
        }
    }
}

#[derive(Clone, Debug)]
pub enum OutputChannel {
    Stdout(EventFormatter),
    AsyncChannel(tokio_mpsc::Sender<Vec<GraphQLTapOutputEvent>>),
}

/// Error type for DNS message parsing
#[derive(Debug)]
pub enum TapExecutorError {
    ConnectionFailure(tokio_tungstenite::tungstenite::Error),
    GraphQLError,
}

#[derive(Debug)]
pub struct TapRunner<'a> {
    url: &'a Url,
    input_patterns: Vec<String>,
    output_patterns: Vec<String>,
    output_channel: &'a OutputChannel,
    format: TapEncodingFormat,
}

impl<'a> TapRunner<'a> {
    pub fn new(
        url: &'a Url,
        input_patterns: Vec<String>,
        output_patterns: Vec<String>,
        output_channel: &'a OutputChannel,
        format: TapEncodingFormat,
    ) -> Self {
        TapRunner {
            url,
            input_patterns,
            output_patterns,
            output_channel,
            format,
        }
    }

    pub async fn run_tap(
        &self,
        interval: i64,
        limit: i64,
        duration_ms: Option<u64>,
        quiet: bool,
    ) -> Result<(), TapExecutorError> {
        let subscription_client = connect_subscription_client((*self.url).clone())
            .await
            .map_err(TapExecutorError::ConnectionFailure)?;

        tokio::pin! {
            let stream = subscription_client.output_events_by_component_id_patterns_subscription(
                self.output_patterns.clone(),
                self.input_patterns.clone(),
                self.format,
                limit,
                interval,
            );
        }

        let start_time = Instant::now();
        let stream_duration = duration_ms
            .map(Duration::from_millis)
            .unwrap_or(Duration::MAX);

        // Loop over the returned results, printing out tap events.
        loop {
            let time_elapsed = start_time.elapsed();
            if time_elapsed >= stream_duration {
                return Ok(());
            }

            let message = timeout(stream_duration - time_elapsed, stream.next()).await;
            match message {
                Ok(Some(Some(res))) => {
                    if let Some(d) = res.data {
                        let output_events: Vec<GraphQLTapOutputEvent> = d
                            .output_events_by_component_id_patterns
                            .into_iter()
                            .filter(|event| {
                                !matches!(
                                    (quiet, event),
                                    (true, GraphQLTapOutputEvent::EventNotification(_))
                                )
                            })
                            .collect();

                        match &self.output_channel {
                            OutputChannel::Stdout(formatter) => {
                                self.output_event_stdout(&output_events, formatter);
                            }
                            OutputChannel::AsyncChannel(sender_tx) => {
                                if let Err(error) = sender_tx.send(output_events).await {
                                    error!("Could not send tap events: {error}");
                                }
                            }
                        }
                    }
                }
                Err(_) =>
                // If the stream times out, that indicates the duration specified by the user
                // has elapsed. We should exit gracefully.
                {
                    return Ok(())
                }
                Ok(_) => return Err(TapExecutorError::GraphQLError),
            }
        }
    }

    #[allow(clippy::print_stdout)]
    fn output_event_stdout(
        &self,
        output_events: &[GraphQLTapOutputEvent],
        formatter: &EventFormatter,
    ) {
        for tap_event in output_events.iter() {
            match tap_event {
                GraphQLTapOutputEvent::Log(ev) => {
                    println!(
                        "{}",
                        formatter.format(
                            ev.component_id.as_ref(),
                            ev.component_kind.as_ref(),
                            ev.component_type.as_ref(),
                            ev.string.as_ref()
                        )
                    );
                }
                GraphQLTapOutputEvent::Metric(ev) => {
                    println!(
                        "{}",
                        formatter.format(
                            ev.component_id.as_ref(),
                            ev.component_kind.as_ref(),
                            ev.component_type.as_ref(),
                            ev.string.as_ref()
                        )
                    );
                }
                GraphQLTapOutputEvent::Trace(ev) => {
                    println!(
                        "{}",
                        formatter.format(
                            ev.component_id.as_ref(),
                            ev.component_kind.as_ref(),
                            ev.component_type.as_ref(),
                            ev.string.as_ref()
                        )
                    );
                }
                #[allow(clippy::print_stderr)]
                GraphQLTapOutputEvent::EventNotification(ev) => {
                    eprintln!("{}", ev.message);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::net::{IpAddr, Ipv4Addr};

    use chrono::Utc;
    use futures_util::sink::SinkExt;
    use futures_util::stream::StreamExt;
    use serde_json::Value;
    use tokio::net::TcpListener;
    use tokio::time::sleep;
    use tokio_tungstenite::accept_async;
    use tokio_tungstenite::tungstenite::Message;

    use portpicker::pick_unused_port;

    use super::*;

    #[tokio::test(start_paused = true)]
    async fn test_async_output_channel() {
        let component_id = "test-component-id";
        let component_type = "test-component-type";
        let message = "test-message";
        let timestamp = Utc::now();
        let string_encoding = "test-str";

        // Start a local WebSocket server to mimic Vector GraphQL API
        let ip_addr = IpAddr::V4(Ipv4Addr::LOCALHOST);
        let port = pick_unused_port(ip_addr);
        let addr = format!("{ip_addr}:{port}");

        let listener = TcpListener::bind(&addr).await.unwrap();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut ws_stream = accept_async(stream).await.unwrap();
            if let Some(Ok(Message::Text(msg))) = ws_stream.next().await {
                let client_init_msg: Value =
                    serde_json::from_str(&msg).expect("Init message should be in JSON format");
                let subscription_id = &client_init_msg["id"];

                let message_to_send = format!(
                    "{{\
                        \"type\":\"data\",\
                        \"id\":{subscription_id},\
                        \"payload\":{{\
                            \"data\":{{\
                                \"outputEventsByComponentIdPatterns\":[{{\
                                    \"__typename\":\"Log\",\
                                    \"componentId\":\"{component_id}\",\
                                    \"componentType\":\"{component_type}\",\
                                    \"componentKind\":\"source\",\
                                    \"message\":\"{message}\",\
                                    \"timestamp\":\"{timestamp}\",\
                                    \"string\":\"{string_encoding}\"\
                                }}]\
                            }}\
                        }}\
                    }}",
                );

                // Send 2 messages to client, mimicking 3 second interval
                loop {
                    ws_stream
                        .send(Message::Text(message_to_send.clone()))
                        .await
                        .unwrap();
                    sleep(Duration::from_secs(3)).await;
                }
            }
        });

        let (output_tx, mut output_rx) = tokio_mpsc::channel(10);
        let url = Url::parse(&format!("ws://{addr}")).unwrap();
        let output_channel = OutputChannel::AsyncChannel(output_tx);

        let tap_runner = TapRunner::new(
            &url,
            vec![],
            vec![],
            &output_channel,
            TapEncodingFormat::Json,
        );
        assert!(tap_runner.run_tap(0, 0, Some(5000), false).await.is_ok());

        let mut num_recv = 0;
        while let Ok(events) = output_rx.try_recv() {
            assert_eq!(events.len(), 1);
            if let GraphQLTapOutputEvent::Log(ev) = &events[0] {
                num_recv += 1;
                assert_eq!(ev.component_id, component_id);
                assert_eq!(ev.component_type, component_type);
                assert_eq!(ev.message, Some(message.to_string()));
                assert_eq!(ev.timestamp, Some(timestamp));
                assert_eq!(ev.string, string_encoding);
            }
        }
        assert_eq!(num_recv, 2);

        server.abort();
    }
}
