use crate::sinks::util::{StreamSink, SinkBuilderExt, BatchSettings, Compression};
use futures::stream::BoxStream;
use crate::event::Event;
use vector_core::partition::Partitioner;
use std::num::NonZeroUsize;
use std::time::Duration;
use futures::StreamExt;
use crate::sinks::elasticsearch::request_builder::ElasticsearchRequestBuilder;
use crate::buffers::Acker;


pub struct NullPartitioner;
impl Partitioner for NullPartitioner {
    type Item = Event;
    type Key = ();

    fn partition(&self, _item: &Self::Item) -> Self::Key {
        ()
    }
}

pub struct ElasticSearchSink {
    batch_timeout: Duration,
    batch_size_bytes: Option<NonZeroUsize>,
    batch_size_events: NonZeroUsize,
    request_builder: ElasticsearchRequestBuilder,
    compression: Compression,
    service: ElasticSearchService,
    acker: Acker,
}

impl ElasticSearchSink {
    pub fn run_inner(mut self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {

        let request_builder_concurrency_limit = NonZeroUsize::new(50);

        let sink = input
            .batched(NullPartitioner,
                self.batch_timeout,
            self.batch_size_events,
            self.batch_settings.size.bytes
            )
            .filter_map(|(_, batch)|async move {
                Some(batch)
            })
            .request_builder(
                request_builder_concurrency_limit,
                self.request_builder,
                self.encoding,
                self.compression,
            ).filter_map(|request| async move {
                match request {
                    Err(e) => {
                        error!("Failed to build S3 request: {:?}.", e);
                        None
                    }
                    Ok(req) => Some(req),
                }
            })
            .into_driver(self.service, self.acker);

        sink.run().await
    }
}



#[async_trait]
impl StreamSink for ElasticSearchSink {
    async fn run(mut self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        self.run_inner(input).await
    }
}
