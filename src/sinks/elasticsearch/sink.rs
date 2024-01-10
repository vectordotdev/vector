use std::fmt;

use vector_lib::lookup::lookup_v2::ConfigValuePath;
use vrl::path::PathPrefix;

use crate::{
    sinks::{
        elasticsearch::{
            encoder::ProcessedEvent, request_builder::ElasticsearchRequestBuilder,
            service::ElasticsearchRequest, BulkAction, ElasticsearchCommonMode,
        },
        prelude::*,
    },
    transforms::metric_to_log::MetricToLog,
};

use super::{ElasticsearchCommon, ElasticsearchConfig};

#[derive(Clone, Eq, Hash, PartialEq)]
pub struct PartitionKey {
    pub index: String,
    pub bulk_action: BulkAction,
}

pub struct ElasticsearchSink<S> {
    pub batch_settings: BatcherSettings,
    pub request_builder: ElasticsearchRequestBuilder,
    pub transformer: Transformer,
    pub service: S,
    pub metric_to_log: MetricToLog,
    pub mode: ElasticsearchCommonMode,
    pub id_key_field: Option<ConfigValuePath>,
}

impl<S> ElasticsearchSink<S> {
    pub fn new(
        common: &ElasticsearchCommon,
        config: &ElasticsearchConfig,
        service: S,
    ) -> crate::Result<Self> {
        let batch_settings = config.batch.into_batcher_settings()?;

        Ok(ElasticsearchSink {
            batch_settings,
            request_builder: common.request_builder.clone(),
            transformer: config.encoding.clone(),
            service,
            metric_to_log: common.metric_to_log.clone(),
            mode: common.mode.clone(),
            id_key_field: config.id_key.clone(),
        })
    }
}

impl<S> ElasticsearchSink<S>
where
    S: Service<ElasticsearchRequest> + Send + 'static,
    S::Future: Send + 'static,
    S::Response: DriverResponse + Send + 'static,
    S::Error: fmt::Debug + Into<crate::Error> + Send,
{
    pub async fn run_inner(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        let mode = self.mode;
        let id_key_field = self.id_key_field.as_ref();
        let transformer = self.transformer.clone();

        input
            .scan(self.metric_to_log, |metric_to_log, event| {
                future::ready(Some(match event {
                    Event::Metric(metric) => metric_to_log.transform_one(metric),
                    Event::Log(log) => Some(log),
                    Event::Trace(_) => {
                        // Although technically this will cause the event to be dropped, due to the sink
                        // config it is not possible to send traces to this sink - so this situation can
                        // never occur. We don't need to emit an `EventsDropped` event.
                        None
                    }
                }))
            })
            .filter_map(|x| async move { x })
            .filter_map(move |log| {
                future::ready(process_log(log, &mode, id_key_field, &transformer))
            })
            .batched(self.batch_settings.as_byte_size_config())
            .request_builder(
                default_request_builder_concurrency_limit(),
                self.request_builder,
            )
            .filter_map(|request| async move {
                match request {
                    Err(error) => {
                        emit!(SinkRequestBuildError { error });
                        None
                    }
                    Ok(req) => Some(req),
                }
            })
            .into_driver(self.service)
            .run()
            .await
    }
}

/// Any `None` values returned from this function will already result in a `TemplateRenderingError`
/// being emitted, so no further `EventsDropped` event needs emitting.
pub(super) fn process_log(
    mut log: LogEvent,
    mode: &ElasticsearchCommonMode,
    id_key_field: Option<&ConfigValuePath>,
    transformer: &Transformer,
) -> Option<ProcessedEvent> {
    let index = mode.index(&log)?;
    let bulk_action = mode.bulk_action(&log)?;

    if let Some(cfg) = mode.as_data_stream_config() {
        cfg.sync_fields(&mut log);
        cfg.remap_timestamp(&mut log);
    };
    let id = if let Some(Value::Bytes(key)) =
        id_key_field.and_then(|key| log.remove((PathPrefix::Event, key)))
    {
        Some(String::from_utf8_lossy(&key).into_owned())
    } else {
        None
    };
    let log = {
        let mut event = Event::from(log);
        transformer.transform(&mut event);
        event.into_log()
    };
    Some(ProcessedEvent {
        index,
        bulk_action,
        log,
        id,
    })
}

#[async_trait]
impl<S> StreamSink<Event> for ElasticsearchSink<S>
where
    S: Service<ElasticsearchRequest> + Send + 'static,
    S::Future: Send + 'static,
    S::Response: DriverResponse + Send + 'static,
    S::Error: fmt::Debug + Into<crate::Error> + Send,
{
    async fn run(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        self.run_inner(input).await
    }
}
