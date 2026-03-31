use std::{borrow::Cow, collections::BTreeMap};

use bytes::Bytes;
use colored::{ColoredString, Colorize};
use prost::Message;
use tokio::{
    sync::mpsc as tokio_mpsc,
    time::{Duration, Instant, timeout},
};
use tokio_stream::StreamExt;
use tokio_util::codec::Encoder;
use url::Url;
use vector_api_client::{
    Client,
    proto::{StreamOutputEventsRequest, StreamOutputEventsResponse},
};
use vector_core::event::Event;

#[derive(Clone, Copy, Debug, clap::ValueEnum)]
pub enum TapEncodingFormat {
    Json,
    Yaml,
    Logfmt,
}

// Note: TapEncodingFormat is kept for CLI compatibility but not used in the gRPC API
// The server now sends proto events directly, which clients serialize as needed

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
    AsyncChannel(tokio_mpsc::Sender<Vec<StreamOutputEventsResponse>>),
}

/// Error type for tap execution
#[derive(Debug)]
pub enum TapExecutorError {
    ConnectionFailure(String),
    GrpcError(String),
    /// Permanent error that should not trigger a reconnect (e.g. invalid arguments).
    Fatal(String),
}

impl TapExecutorError {
    pub fn is_fatal(&self) -> bool {
        matches!(self, TapExecutorError::Fatal(_))
    }
}

impl From<vector_api_client::Error> for TapExecutorError {
    fn from(err: vector_api_client::Error) -> Self {
        if err.is_fatal() {
            TapExecutorError::Fatal(format!("{}", err))
        } else {
            TapExecutorError::GrpcError(format!("{}", err))
        }
    }
}

#[derive(Debug)]
pub struct TapRunner<'a> {
    url: &'a Url,
    input_patterns: Vec<String>,
    output_patterns: Vec<String>,
    output_channel: &'a OutputChannel,
}

impl<'a> TapRunner<'a> {
    pub fn new(
        url: &'a Url,
        input_patterns: Vec<String>,
        output_patterns: Vec<String>,
        output_channel: &'a OutputChannel,
    ) -> Self {
        TapRunner {
            url,
            input_patterns,
            output_patterns,
            output_channel,
        }
    }

    pub async fn run_tap(
        &self,
        interval: i64,
        limit: i64,
        duration_ms: Option<u64>,
        quiet: bool,
    ) -> Result<(), TapExecutorError> {
        let uri = self
            .url
            .as_str()
            .parse()
            .map_err(|e| TapExecutorError::Fatal(format!("Invalid URL: {e}")))?;
        let mut client = Client::new(uri);
        client.connect().await?;
        self.run_tap_with_client(client, interval, limit, duration_ms, quiet)
            .await
    }

    /// Run tap using a pre-connected client (avoids an extra connection round-trip when the
    /// caller has already connected and health-checked the client).
    pub async fn run_tap_with_client(
        &self,
        mut client: Client,
        interval: i64,
        limit: i64,
        duration_ms: Option<u64>,
        quiet: bool,
    ) -> Result<(), TapExecutorError> {
        let request = StreamOutputEventsRequest {
            outputs_patterns: self.output_patterns.clone(),
            inputs_patterns: self.input_patterns.clone(),
            limit: limit as i32,
            interval_ms: interval as i32,
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
                            Some(
                                vector_api_client::proto::stream_output_events_response::Event::Notification(
                                    _
                                )
                            )
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

    /// Convert and serialize a protobuf EventWrapper to the requested format
    fn serialize_event(
        event_wrapper: &vector_api_client::proto::event::EventWrapper,
        format: TapEncodingFormat,
    ) -> Result<String, String> {
        // INTENTIONAL round-trip through protobuf bytes: `vector_api_client` compiles
        // `event.proto` independently to avoid taking a dependency on `vector_core` (a large
        // crate with many features). Both types share the same proto schema, so encoding one
        // and decoding the other is always safe. If the schemas ever diverge this will surface
        // as a runtime decode error.
        let bytes = event_wrapper.encode_to_vec();

        let core_event_wrapper =
            vector_core::event::proto::EventWrapper::decode(Bytes::from(bytes))
                .map_err(|e| format!("Failed to decode event: {}", e))?;

        // Convert to vector-core Event (which has Serialize)
        let event: Event = core_event_wrapper.into();

        // Serialize based on format
        match format {
            TapEncodingFormat::Json => serde_json::to_string(&event)
                .map_err(|e| format!("JSON serialization failed: {}", e)),
            TapEncodingFormat::Yaml => serde_yaml::to_string(&event)
                .map_err(|e| format!("YAML serialization failed: {}", e)),
            TapEncodingFormat::Logfmt => {
                // For logfmt, we need to extract the log event and serialize it
                match event {
                    Event::Log(log_event) => {
                        let mut serializer =
                            codecs::encoding::format::LogfmtSerializerConfig.build();
                        let mut bytes = bytes::BytesMut::new();
                        // Wrap the LogEvent back into Event for the serializer
                        serializer
                            .encode(Event::Log(log_event), &mut bytes)
                            .map_err(|e| format!("Logfmt serialization failed: {}", e))?;
                        String::from_utf8(bytes.to_vec())
                            .map_err(|e| format!("UTF-8 conversion failed: {}", e))
                    }
                    Event::Metric(_) => {
                        Err("logfmt format is only supported for log events".to_string())
                    }
                    Event::Trace(_) => {
                        Err("logfmt format is only supported for log events".to_string())
                    }
                }
            }
        }
    }

    #[allow(clippy::print_stdout)]
    fn output_event_stdout(
        &self,
        output_event: &StreamOutputEventsResponse,
        formatter: &EventFormatter,
    ) {
        use vector_api_client::proto::stream_output_events_response::Event as OutputEventType;

        match &output_event.event {
            Some(OutputEventType::TappedEvent(ev)) => {
                // Format the proto event for display
                let encoded_string = if let Some(ref event_wrapper) = ev.event {
                    match Self::serialize_event(event_wrapper, formatter.format) {
                        Ok(s) => s,
                        Err(e) => {
                            error!(message = "Failed to serialize event.", error = %e);
                            format!("{:?}", event_wrapper)
                        }
                    }
                } else {
                    "No event data".to_string()
                };

                println!(
                    "{}",
                    formatter.format(
                        &ev.component_id,
                        &ev.component_kind,
                        &ev.component_type,
                        &encoded_string
                    )
                );
            }
            #[allow(clippy::print_stderr)]
            Some(OutputEventType::Notification(ev)) => {
                eprintln!("{}", ev.message);
            }
            None => {
                error!("Received StreamOutputEventsResponse with no event data");
            }
        }
    }
}
