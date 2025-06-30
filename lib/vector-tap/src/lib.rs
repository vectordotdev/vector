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
