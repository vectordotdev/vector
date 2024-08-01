use crate::sinks::{
    greptimedb::logs::http_request_builder::{
        GreptimeDBLogsHttpRequestBuilder, KeyPartitioner, PartitionKey,
    },
    prelude::*,
    util::http::HttpRequest,
};

pub struct LogsSinkSetting {
    pub dbname: Template,
    pub table: Template,
    pub pipeline_name: Template,
    pub pipeline_version: Option<Template>,
}

/// A sink that ingests logs into GreptimeDB.
pub struct GreptimeDBLogsHttpSink<S> {
    batcher_settings: BatcherSettings,
    service: S,
    request_builder: GreptimeDBLogsHttpRequestBuilder,
    logs_sink_setting: LogsSinkSetting,
}

impl<S> GreptimeDBLogsHttpSink<S>
where
    S: Service<HttpRequest<PartitionKey>> + Send + 'static,
    S::Future: Send + 'static,
    S::Response: DriverResponse + Send + 'static,
    S::Error: std::fmt::Debug + Into<crate::Error> + Send,
{
    pub const fn new(
        batcher_settings: BatcherSettings,
        service: S,
        request_builder: GreptimeDBLogsHttpRequestBuilder,
        logs_sink_setting: LogsSinkSetting,
    ) -> Self {
        Self {
            batcher_settings,
            service,
            request_builder,
            logs_sink_setting,
        }
    }

    async fn run_inner(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        let batcher_settings = self.batcher_settings;
        input
            .batched_partitioned(
                KeyPartitioner::new(
                    self.logs_sink_setting.dbname,
                    self.logs_sink_setting.table,
                    self.logs_sink_setting.pipeline_name,
                    self.logs_sink_setting.pipeline_version,
                ),
                || batcher_settings.as_byte_size_config(),
            )
            .filter_map(|(key, batch)| async move { key.map(move |k| (k, batch)) })
            .request_builder(
                default_request_builder_concurrency_limit(),
                self.request_builder,
            )
            .filter_map(|request| async {
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

#[async_trait::async_trait]
impl<S> StreamSink<Event> for GreptimeDBLogsHttpSink<S>
where
    S: Service<HttpRequest<PartitionKey>> + Send + 'static,
    S::Future: Send + 'static,
    S::Response: DriverResponse + Send + 'static,
    S::Error: std::fmt::Debug + Into<crate::Error> + Send,
{
    async fn run(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        self.run_inner(input).await
    }
}
