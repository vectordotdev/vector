use futures_util::{
    stream::{self, BoxStream},
    StreamExt,
};
use vector_core::event::Event;
use vector_core::sink::StreamSink;
use vector_core::stream::BatcherSettings;

use super::request_builder::BigqueryRequestBuilder;
use super::service::BigqueryService;
use crate::sinks::prelude::SinkRequestBuildError;
use crate::sinks::util::builder::SinkBuilderExt;

pub struct BigquerySink {
    pub service: BigqueryService,
    pub batcher_settings: BatcherSettings,
    pub request_builder: BigqueryRequestBuilder,
}

impl BigquerySink {
    async fn run_inner(self: Box<BigquerySink>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        input
            .batched(self.batcher_settings.as_byte_size_config())
            .incremental_request_builder(self.request_builder)
            .flat_map(stream::iter)
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
            .protocol("gRPC")
            .run()
            .await
    }
}

#[async_trait::async_trait]
impl StreamSink<Event> for BigquerySink {
    async fn run(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        self.run_inner(input).await
    }
}
