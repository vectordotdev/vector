use crate::event::Event;
use futures_util::{
    StreamExt,
    stream::{self, BoxStream},
};
use vector_lib::sink::StreamSink;
use vector_lib::stream::BatcherSettings;

use super::request_builder::BigqueryRequestBuilder;
use super::service::{BigqueryRetryLogic, BigqueryService};
use crate::sinks::prelude::SinkRequestBuildError;
use crate::sinks::util::builder::SinkBuilderExt;
use crate::sinks::util::service::Svc;

pub struct BigquerySink {
    pub service: Svc<BigqueryService, BigqueryRetryLogic>,
    pub batcher_settings: BatcherSettings,
    pub request_builder: BigqueryRequestBuilder,
}

impl BigquerySink {
    async fn run_inner(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
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
