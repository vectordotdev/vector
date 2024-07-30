use crate::sinks::{
    greptimedb::logs::http_request_builder::{
        GreptimeDBLogsHttpRequestBuilder, KeyPartitioner, PartitionKey,
    },
    prelude::*,
    util::http::HttpRequest,
};

/// A sink that ingests logs into GreptimeDB.
pub struct GreptimeDBLogsHttpSink<S> {
    batcher_settings: BatcherSettings,
    service: S,
    dbname: Template,
    table: Template,
    pipeline_name: Template,
    pipeline_version: Option<Template>,
    request_builder: GreptimeDBLogsHttpRequestBuilder,
    protocol: String,
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
        dbname: Template,
        table: Template,
        pipeline_name: Template,
        pipeline_version: Option<Template>,
        request_builder: GreptimeDBLogsHttpRequestBuilder,
        protocol: String,
    ) -> Self {
        Self {
            batcher_settings,
            service,
            dbname,
            table,
            pipeline_name,
            pipeline_version,
            request_builder,
            protocol,
        }
    }

    async fn run_inner(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        let batcher_settings = self.batcher_settings;
        input
            .batched_partitioned(
                KeyPartitioner::new(
                    self.dbname,
                    self.table,
                    self.pipeline_name,
                    self.pipeline_version,
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
            .protocol(self.protocol)
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
