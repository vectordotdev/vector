use futures::{stream::BoxStream, StreamExt};
use tower::{Service, ServiceBuilder};
use vector_core::{
    event::Event,
    sink::StreamSink,
    stream::{BatcherSettings, DriverResponse},
};

use crate::{
    codecs::Transformer, internal_events::SinkRequestBuildError,
    sinks::util::builder::SinkBuilderExt, sinks::util::Compression,
};

use super::{
    encoder::AppsignalEncoder,
    request_builder::{AppsignalRequest, AppsignalRequestBuilder},
};

pub(super) struct AppsignalSink<S> {
    pub(super) service: S,
    pub(super) compression: Compression,
    pub(super) transformer: Transformer,
    pub(super) batch_settings: BatcherSettings,
}

impl<S> AppsignalSink<S>
where
    S: Service<AppsignalRequest> + Send + 'static,
    S::Future: Send + 'static,
    S::Response: DriverResponse + Send + 'static,
    S::Error: std::fmt::Debug + Into<crate::Error> + Send,
{
    pub(super) async fn run_inner(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        let service = ServiceBuilder::new().service(self.service);

        input
            .batched(self.batch_settings.into_byte_size_config())
            .request_builder(
                None,
                AppsignalRequestBuilder {
                    compression: self.compression,
                    encoder: AppsignalEncoder {
                        transformer: self.transformer.clone(),
                    },
                },
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
            .into_driver(service)
            .run()
            .await
    }
}

#[async_trait::async_trait]
impl<S> StreamSink<Event> for AppsignalSink<S>
where
    S: Service<AppsignalRequest> + Send + 'static,
    S::Future: Send + 'static,
    S::Response: DriverResponse + Send + 'static,
    S::Error: std::fmt::Debug + Into<crate::Error> + Send,
{
    async fn run(
        self: Box<Self>,
        input: futures_util::stream::BoxStream<'_, Event>,
    ) -> Result<(), ()> {
        self.run_inner(input).await
    }
}
