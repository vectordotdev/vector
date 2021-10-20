use std::{fmt, num::NonZeroUsize};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use futures_util::{future, stream::BoxStream, StreamExt};
use tower::Service;
use vector_core::{
    config::log_schema,
    event::{Event, EventFinalizers, EventStatus, Finalizable, LogEvent, Value},
    partition::NullPartitioner,
    sink::StreamSink,
    stream::BatcherSettings,
    ByteSizeOf,
};

use crate::{
    config::SinkContext,
    http::HttpClient,
    sinks::{
        splunk_hec::common::render_template_string,
        util::{Compression, SinkBuilderExt},
    },
    template::Template,
};

use super::service::{HecLogsRequest, HecLogsRequestBuilder};

pub struct HecLogsSink<S> {
    pub context: SinkContext,
    pub service: S,
    pub request_builder: HecLogsRequestBuilder,
    pub batch_settings: BatcherSettings,
    pub source: Option<Template>,
}

impl<S> HecLogsSink<S>
where
    S: Service<HecLogsRequest> + Send + 'static,
    S::Future: Send + 'static,
    S::Response: AsRef<EventStatus> + Send + 'static,
    S::Error: fmt::Debug + Into<crate::Error> + Send,
{
    async fn run_inner(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        // is clone needed here?
        let source = self.source.clone();

        let builder_limit = NonZeroUsize::new(64);
        let sink = input
            .map(|event| event.into_log())
            .filter_map(move |log| {
                future::ready(process_log(
                    log,
                    None,
                    source.as_ref(),
                    None,
                    "".to_string(),
                    Vec::new(),
                ))
            })
            .batched(NullPartitioner::new(), self.batch_settings)
            .request_builder(builder_limit, self.request_builder)
            .filter_map(|request| async move {
                match request {
                    Err(e) => {
                        error!("Failed to build HEC Logs request: {:?}.", e);
                        None
                    }
                    Ok(req) => Some(req),
                }
            })
            .into_driver(self.service, self.context.acker());

        sink.run().await
    }
}

#[async_trait]
impl<S> StreamSink for HecLogsSink<S>
where
    S: Service<HecLogsRequest> + Send + 'static,
    S::Future: Send + 'static,
    S::Response: AsRef<EventStatus> + Send + 'static,
    S::Error: fmt::Debug + Into<crate::Error> + Send,
{
    async fn run(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        self.run_inner(input).await
    }
}

#[derive(PartialEq, Default, Clone, Debug)]
pub struct ProcessedEvent {
    pub log: LogEvent,
    pub sourcetype: Option<String>,
    pub source: Option<String>,
    pub index: Option<String>,
    pub host: Option<Value>,
    pub timestamp: f64,
    pub fields: LogEvent,
}

impl Finalizable for ProcessedEvent {
    fn take_finalizers(&mut self) -> EventFinalizers {
        self.log.metadata_mut().take_finalizers()
    }
}

impl ByteSizeOf for ProcessedEvent {
    fn allocated_bytes(&self) -> usize {
        self.log.allocated_bytes()
            + self.sourcetype.allocated_bytes()
            + self.source.allocated_bytes()
            + self.index.allocated_bytes()
            + self.host.allocated_bytes()
            + self.timestamp.allocated_bytes()
            + self.fields.allocated_bytes()
    }
}

fn process_log(
    mut log: LogEvent,
    sourcetype: Option<&Template>,
    source: Option<&Template>,
    index: Option<&Template>,
    host_key: String,
    indexed_fields: Vec<String>,
) -> Option<ProcessedEvent> {
    println!("[sink::process_log] {:?}", log);
    let sourcetype = sourcetype
        .and_then(|sourcetype| render_template_string(sourcetype, &log, "sourcetype"));
    println!("[sink::process_log] {:?}", sourcetype);

    let source = source
        .and_then(|source| render_template_string(source, &log, "source"));

    let index = index
        .and_then(|index| render_template_string(index, &log, "index"));

    let host = log.get(host_key.clone()).cloned();

    let timestamp = match log.remove(log_schema().timestamp_key()) {
        Some(Value::Timestamp(ts)) => ts,
        _ => chrono::Utc::now(),
    };
    let timestamp = (timestamp.timestamp_millis() as f64) / 1000f64;

    let fields = indexed_fields
        .iter()
        .filter_map(|field| log.get(field).map(|value| (field, value.clone())))
        .collect::<LogEvent>();

    Some(ProcessedEvent {
        log,
        sourcetype,
        source,
        index,
        host,
        timestamp,
        fields,
    })
}
