use crate::sinks::greptimedb::logs::http_reuqest_builder::{
    GreptimeDBLogsHttpRequestBuilder, PartitionKey,
};
use crate::sinks::prelude::*;
use crate::sinks::util::http::HttpRequest;

pub struct GreptimeDBLogsHttpSink<S> {
    batcher_settings: BatcherSettings,
    service: S,
    db: String,
    table: String,
    pipeline_name: String,
    pipeline_version: Option<String>,
    request_builder: GreptimeDBLogsHttpRequestBuilder,
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
        db: String,
        table: String,
        pipeline_name: String,
        pipeline_version: Option<String>,
        request_builder: GreptimeDBLogsHttpRequestBuilder,
    ) -> Self {
        Self {
            batcher_settings,
            service,
            db,
            table,
            pipeline_name,
            pipeline_version,
            request_builder,
        }
    }

    async fn run_inner(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        let batcher_settings = self.batcher_settings;
        input
            .batched_partitioned(
                PartitionKey {
                    db: self.db,
                    table: self.table,
                    pipeline_name: self.pipeline_name,
                    pipeline_version: self.pipeline_version,
                },
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
