#![deny(warnings)]

#[macro_use]
extern crate tracing;

pub mod controller;
pub mod notification;
pub mod topology;

use std::{borrow::Cow, collections::BTreeMap};

use colored::{ColoredString, Colorize};
use tokio::{
    sync::mpsc as tokio_mpsc,
    time::{Duration, Instant, timeout},
};
use tokio_stream::StreamExt;
use url::Url;
use vector_api_client::{
    Client,
    proto::{EventEncoding, OutputEvent, OutputEventsRequest},
};

#[derive(Clone, Copy, Debug, clap::ValueEnum)]
pub enum TapEncodingFormat {
    Json,
    Yaml,
    Logfmt,
}

impl From<TapEncodingFormat> for EventEncoding {
    fn from(format: TapEncodingFormat) -> Self {
        match format {
            TapEncodingFormat::Json => EventEncoding::Json,
            TapEncodingFormat::Yaml => EventEncoding::Yaml,
            TapEncodingFormat::Logfmt => EventEncoding::Logfmt,
        }
    }
}

impl From<TapEncodingFormat> for i32 {
    fn from(format: TapEncodingFormat) -> Self {
        EventEncoding::from(format) as i32
    }
}

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
    AsyncChannel(tokio_mpsc::Sender<Vec<OutputEvent>>),
}

/// Error type for tap execution
#[derive(Debug)]
pub enum TapExecutorError {
    ConnectionFailure(String),
    GrpcError(String),
}

impl From<vector_api_client::Error> for TapExecutorError {
    fn from(err: vector_api_client::Error) -> Self {
        TapExecutorError::GrpcError(format!("{}", err))
    }
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
        let mut client = Client::new(self.url.as_str()).await?;
        client.connect().await?;

        let request = OutputEventsRequest {
            outputs_patterns: self.output_patterns.clone(),
            inputs_patterns: self.input_patterns.clone(),
            limit: limit as i32,
            interval_ms: interval as i32,
            encoding: self.format.into(),
        };

        let mut stream = client.stream_output_events(request).await?;

        let start_time = Instant::now();
        let stream_duration = duration_ms
            .map(Duration::from_millis)
            .unwrap_or(Duration::MAX);

        // Loop over the returned results, processing tap events
        loop {
            let time_elapsed = start_time.elapsed();
            if time_elapsed >= stream_duration {
                return Ok(());
            }

            let message = timeout(stream_duration - time_elapsed, stream.next()).await;
            match message {
                Ok(Some(Ok(output_event))) => {
                    // Filter out notifications if quiet mode is enabled
                    if quiet
                        && matches!(
                            output_event.event,
                            Some(vector_api_client::proto::output_event::Event::Notification(
                                _
                            ))
                        )
                    {
                        continue;
                    }

                    match &self.output_channel {
                        OutputChannel::Stdout(formatter) => {
                            self.output_event_stdout(&output_event, formatter);
                        }
                        OutputChannel::AsyncChannel(sender_tx) => {
                            if let Err(error) = sender_tx.send(vec![output_event]).await {
                                error!("Could not send tap events: {error}");
                            }
                        }
                    }
                }
                Err(_) =>
                // If the stream times out, that indicates the duration specified by the user
                // has elapsed. We should exit gracefully.
                {
                    return Ok(());
                }
                Ok(None) => {
                    return Err(TapExecutorError::GrpcError(
                        "Stream ended unexpectedly".to_string(),
                    ));
                }
                Ok(Some(Err(err))) => return Err(TapExecutorError::from(err)),
            }
        }
    }

    #[allow(clippy::print_stdout)]
    fn output_event_stdout(&self, output_event: &OutputEvent, formatter: &EventFormatter) {
        use vector_api_client::proto::output_event::Event;

        match &output_event.event {
            Some(Event::Log(ev)) => {
                println!(
                    "{}",
                    formatter.format(
                        &ev.component_id,
                        &ev.component_kind,
                        &ev.component_type,
                        &ev.encoded_string
                    )
                );
            }
            Some(Event::Metric(ev)) => {
                println!(
                    "{}",
                    formatter.format(
                        &ev.component_id,
                        &ev.component_kind,
                        &ev.component_type,
                        &ev.encoded_string
                    )
                );
            }
            Some(Event::Trace(ev)) => {
                println!(
                    "{}",
                    formatter.format(
                        &ev.component_id,
                        &ev.component_kind,
                        &ev.component_type,
                        &ev.encoded_string
                    )
                );
            }
            #[allow(clippy::print_stderr)]
            Some(Event::Notification(ev)) => {
                eprintln!("{}", ev.message);
            }
            None => {
                error!("Received OutputEvent with no event data");
            }
        }
    }
}
