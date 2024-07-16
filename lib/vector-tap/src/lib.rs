#![deny(warnings)]

extern crate tracing;

use std::time::{Duration, Instant};
use std::{borrow::Cow, collections::BTreeMap};

use colored::{ColoredString, Colorize};
use snafu::Snafu;
use tokio::time::timeout;
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

/// Error type for DNS message parsing
#[derive(Debug, Snafu)]
pub enum TapExecutorError {
    #[snafu(display("[tap] Couldn't connect to API via WebSockets"))]
    ConnectionFailure,
    GraphQLError,
}

#[derive(Debug)]
pub struct TapRunner<'a> {
    url: &'a Url,
    input_patterns: Vec<String>,
    output_patterns: Vec<String>,
    formatter: &'a EventFormatter,
    format: TapEncodingFormat,
}

impl<'a> TapRunner<'a> {
    pub fn new(
        url: &'a Url,
        input_patterns: Vec<String>,
        output_patterns: Vec<String>,
        formatter: &'a EventFormatter,
        format: TapEncodingFormat,
    ) -> Self {
        TapRunner {
            url,
            input_patterns,
            output_patterns,
            formatter,
            format,
        }
    }

    #[allow(clippy::print_stdout)]
    #[allow(clippy::print_stderr)]
    pub async fn run_tap(
        &self,
        interval: i64,
        limit: i64,
        duration_ms: Option<u64>,
        quiet: bool,
    ) -> Result<(), TapExecutorError> {
        let subscription_client = connect_subscription_client((*self.url).clone())
            .await
            .map_err(|error| {
                eprintln!("[tap] Couldn't connect to API via WebSockets: {error}");
                TapExecutorError::ConnectionFailure
            })?;

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
                        for tap_event in d.output_events_by_component_id_patterns.iter() {
                            match tap_event {
                                GraphQLTapOutputEvent::Log(ev) => {
                                    println!(
                                        "{}",
                                        self.formatter.format(
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
                                        self.formatter.format(
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
                                        self.formatter.format(
                                            ev.component_id.as_ref(),
                                            ev.component_kind.as_ref(),
                                            ev.component_type.as_ref(),
                                            ev.string.as_ref()
                                        )
                                    );
                                }
                                GraphQLTapOutputEvent::EventNotification(ev) => {
                                    if !quiet {
                                        eprintln!("{}", ev.message);
                                    }
                                }
                            }
                        }
                    }
                }
                Err(_) => {
                    // If the stream times out, that indicates the duration specified by the user
                    // has elapsed. We should exit gracefully.
                    return Ok(());
                }
                Ok(_) => return Err(TapExecutorError::GraphQLError),
            }
        }
    }
}
