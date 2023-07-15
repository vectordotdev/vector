use bytes::Bytes;
use codecs::{encoding::Framer, JsonSerializerConfig, NewlineDelimitedEncoderConfig};
use futures_util::stream;

use super::service::{ClickhouseRequest, ClickhouseRetryLogic, ClickhouseService};
use crate::sinks::util::IncrementalRequestBuilder;
use crate::{internal_events::SinkRequestBuildError, sinks::prelude::*};
use vector_core::event::{EventFinalizers, Event, LogEvent, TraceEvent, Metric};

pub struct ClickhouseSink {
    batch_settings: BatcherSettings,
    // encoding: (Transformer, Encoder<Framer>),
    service: Svc<ClickhouseService, ClickhouseRetryLogic>,
    protocol: &'static str,
    request_builder: ClickhouseRequestBuilder,
}

struct ClickhouseRequestBuilder {
    compression: Compression,
    transformer: Transformer,
    encoder: Encoder<Framer>,
}

impl ClickhouseSink {
    pub fn new(
        batch_settings: BatcherSettings,
        compression: Compression,
        transformer: Transformer,
        service: Svc<ClickhouseService, ClickhouseRetryLogic>,
        protocol: &'static str,
    ) -> Self {
        Self {
            batch_settings,
            service,
            protocol,
            request_builder: ClickhouseRequestBuilder {
                compression: compression,
                transformer: transformer,
                encoder: Encoder::<Framer>::new(
                    NewlineDelimitedEncoderConfig::default().build().into(),
                    JsonSerializerConfig::default().build().into(),
                ),
            },
        }
    }

    async fn run_inner(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        input
            //.normalized_with_default::<ClickHouseNormalizer>()
            .batched(self.batch_settings.into_byte_size_config())
            .incremental_request_builder(self.request_builder)
            .flat_map(stream::iter)
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
impl StreamSink<Event> for ClickhouseSink {
    async fn run(
        self: Box<Self>,
        input: futures_util::stream::BoxStream<'_, Event>,
    ) -> Result<(), ()> {
        self.run_inner(input).await
    }
}

impl IncrementalRequestBuilder<Vec<Event>> for ClickhouseRequestBuilder {
    type Metadata = (EventFinalizers, RequestMetadata);
    type Payload = Bytes;
    type Request = ClickhouseRequest;
    type Error = std::io::Error;

    fn encode_events_incremental(&mut self, mut input: Vec<Event>,
    ) -> Vec<Result<(Self::Metadata, Self::Payload), Self::Error>> {
        let mut results = Vec::with_capacity(input.len());
        let mut metrics: Vec<Metric> = Vec::new();
        let mut traces: Vec<TraceEvent> = Vec::new();
        let mut logs: Vec<LogEvent> = Vec::new();

        let mut request_metadata_builder = RequestMetadataBuilder::default();

        for event in input.drain(..) {
            transformer.transform(&mut event);
            match event {
                Event::Log(log) => logs.push(log),
                Event::Metric(metric) => metrics.push(metric),
                Event::Trace(trace) => traces.push(trace),
            };
        }

        {
            let mut request_buf = Vec::new();
            let mut finalizers = EventFinalizers::default();
            for log in logs {
                encoder.encode(&log, &mut request_buf);
                finalizers.merge(log.take_finalizers());
                request_metadata_builder.track_event(log);
            }
            let encode_result = EncodeResult::uncompressed(request_buf);
            let request_metadata = request_metadata_builder.build(&encode_result);
            results.push(Ok((finalizers,request_metadata), request_buf));
        }

        results
    }

    fn build_request( &mut self, finalizers_and_metadata: Self::Metadata, payload: Self::Payload) -> Self::Request {
        let (finalizers, metadata) = finalizers_and_metadata;
        ClickhouseRequest {
            body: payload,
            compression: self.compression,
            finalizers: finalizers,
            metadata: metadata,
        }
    }
}
