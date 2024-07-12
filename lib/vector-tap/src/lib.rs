#![deny(warnings)]

#[macro_use]
extern crate tracing;

use std::{borrow::Cow, collections::BTreeMap};
use std::time::{Duration, Instant};

use colored::{ColoredString, Colorize};
use snafu::Snafu;
use tokio::sync::mpsc as tokio_mpsc;
use tokio::time::timeout;
use tokio_stream::StreamExt;
use url::Url;

use vector_api_client::{
    connect_subscription_client,
    gql::{
        output_events_by_component_id_patterns_subscription::OutputEventsByComponentIdPatternsSubscriptionOutputEventsByComponentIdPatterns as GraphQLTapOutputEvents,
        TapEncodingFormat,
        TapSubscriptionExt,
    },
};

#[derive(Clone)]
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

pub enum OutputChannel {
    Stdout(EventFormatter),
    AsyncChannel(tokio_mpsc::Sender<Vec<GraphQLTapOutputEvents>>),
}

/// Error type for DNS message parsing
#[derive(Debug, Snafu)]
pub enum TapExecutorError {
    #[snafu(display("[tap] Couldn't connect to API via WebSockets"))]
    ConnectionFailure,
    NoEventsFound,
}

#[allow(clippy::too_many_arguments)]
pub async fn exec_tap(
    url: Url,
    interval: i64,
    limit: i64,
    duration_ms: Option<u64>,
    input_patterns: Vec<String>,
    output_patterns: Vec<String>,
    format: TapEncodingFormat,
    output_channel: OutputChannel,
    quiet: bool,
) -> Result<(), TapExecutorError> {
    let subscription_client = match connect_subscription_client(url.clone()).await {
        Ok(c) => c,
        Err(e) => {
            #[allow(clippy::print_stderr)]
            {
                eprintln!("[tap] Couldn't connect to API via WebSockets: {}", e);
            }
            return Err(TapExecutorError::ConnectionFailure);
        }
    };

    tokio::pin! {
        let stream = subscription_client.output_events_by_component_id_patterns_subscription(
            output_patterns.clone(),
            input_patterns.clone(),
            format,
            limit,
            interval,
        );
    }

    let start_time = Instant::now();
    let stream_duration =
        duration_ms
            .map(Duration::from_millis)
            .unwrap_or(Duration::MAX);

    // Loop over the returned results, printing out tap events.
    #[allow(clippy::print_stdout)]
    #[allow(clippy::print_stderr)]
    loop {
        let time_elapsed = start_time.elapsed();
        if time_elapsed >= stream_duration {
            return Ok(());
        }

        let message = timeout(stream_duration - time_elapsed, stream.next()).await;
        match message {
            Ok(Some(Some(res))) => {
                if let Some(d) = res.data {
                    match &output_channel {
                        OutputChannel::Stdout(formatter) => {
                            for tap_event in d.output_events_by_component_id_patterns.iter() {
                                match tap_event {
                                    GraphQLTapOutputEvents::Log(ev) => {
                                        println!("{}", formatter.format(ev.component_id.as_ref(), ev.component_kind.as_ref(), ev.component_type.as_ref(), ev.string.as_ref()));
                                    }
                                    GraphQLTapOutputEvents::Metric(ev) => {
                                        println!("{}", formatter.format(ev.component_id.as_ref(), ev.component_kind.as_ref(), ev.component_type.as_ref(), ev.string.as_ref()));
                                    }
                                    GraphQLTapOutputEvents::Trace(ev) => {
                                        println!("{}", formatter.format(ev.component_id.as_ref(), ev.component_kind.as_ref(), ev.component_type.as_ref(), ev.string.as_ref()));
                                    }
                                    GraphQLTapOutputEvents::EventNotification(ev) => {
                                        if !quiet {
                                            eprintln!("{}", ev.message);
                                        }
                                    }
                                }
                            }
                        }
                        OutputChannel::AsyncChannel (sender_tx) => {
                            if sender_tx.send(d.output_events_by_component_id_patterns).await.is_err() {
                                debug!("Could not send events");
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
            Ok(_) => return Err(TapExecutorError::NoEventsFound)
        }
    }
}

