use crate::sinks::greptimedb::request::GreptimeDBRequest;
use crate::sinks::{
    greptimedb::{GreptimeDBRetryLogic, GreptimeDBService},
    prelude::*,
};

pub struct GreptimeDBLogsSink {
    pub(super) service: Svc<GreptimeDBService, GreptimeDBRetryLogic>,
    pub(super) batch_settings: BatcherSettings,
    pub(super) table: String,
}

impl GreptimeDBLogsSink {
    async fn run_inner(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        input
            .map(|event| event.into_log())
            .batched(self.batch_settings.as_byte_size_config())
            .map(|event| GreptimeDBRequest::from_logs(event, self.table.clone()))
            .into_driver(self.service)
            .protocol("grpc")
            .run()
            .await
    }
}

#[async_trait]
impl StreamSink<Event> for GreptimeDBLogsSink {
    async fn run(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        self.run_inner(input).await
    }
}
