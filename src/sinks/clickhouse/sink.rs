use vector_core::event::{EventFinalizers, Event};

use crate::sinks::util::IncrementalRequestBuilder;
use crate::sinks::util::buffer::metrics::MetricNormalizer;
use crate::{internal_events::SinkRequestBuildError, sinks::prelude::*};

use super::service::{ClickhouseRequest, ClickhouseRetryLogic, ClickhouseService};
use super::normalizer::ClickHouseMetricsNormalizer;

use bytes::{Bytes, BytesMut};
use codecs::{encoding::Framer, JsonSerializerConfig, NewlineDelimitedEncoderConfig};
use futures_util::stream;

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
    normalizer: MetricNormalizer<ClickHouseMetricsNormalizer>,
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
                normalizer: MetricNormalizer::from(ClickHouseMetricsNormalizer::default()),
            },
        }
    }

    async fn run_inner(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        input
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
        let mut results = Vec::with_capacity(3);

        let mut metrics_buf = BytesMut::new();
        let mut traces_buf = BytesMut::new();
        let mut logs_buf = BytesMut::new();
        let mut metrics_finalizers = EventFinalizers::default();
        let mut traces_finalizers = EventFinalizers::default();
        let mut logs_finalizers = EventFinalizers::default();

        let request_metadata_builder = RequestMetadataBuilder::default();

        for mut event in input.drain(..) {
            self.transformer.transform(&mut event);
            match event {
                Event::Log(mut log) => {
                    logs_finalizers.merge(log.take_finalizers());
                    self.encoder.serialize(Event::Log(log), &mut logs_buf).expect("encoding is infallible");
                },
                Event::Trace(mut trace) => {
                    traces_finalizers.merge(trace.take_finalizers());
                    self.encoder.serialize(Event::Trace(trace), &mut traces_buf).expect("encoding is infallible");
                },
                Event::Metric(mut metric) => {
                    metrics_finalizers.merge(metric.take_finalizers());
                    if let Some(normalized) = self.normalizer.normalize(metric) {
                        self.encoder.serialize(Event::Metric(normalized), &mut metrics_buf).expect("encoding is infallible");
                    }
                },
            };
        }

        if metrics_buf.len() > 0 {
            let metrics_encoded = EncodeResult::uncompressed(metrics_buf);
            let metrics_metadata = request_metadata_builder.build(&metrics_encoded);
            results.push(Ok(((metrics_finalizers, metrics_metadata), metrics_encoded.into_payload().freeze())))
        }
        if traces_buf.len() > 0 {
            let traces_encoded = EncodeResult::uncompressed(traces_buf);
            let traces_metadata = request_metadata_builder.build(&traces_encoded);
            results.push(Ok(((traces_finalizers, traces_metadata), traces_encoded.into_payload().freeze())))
        }
        if logs_buf.len() > 0 {
            let logs_encoded = EncodeResult::uncompressed(logs_buf);
            let logs_metadata = request_metadata_builder.build(&logs_encoded);
            results.push(Ok(((logs_finalizers, logs_metadata), logs_encoded.into_payload().freeze())))
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
