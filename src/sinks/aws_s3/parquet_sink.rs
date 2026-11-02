//! S3 sink variant that encodes Parquet objects incrementally.
//!
//! Used when both `batch_encoding` and `encoded_batch` are configured. Events
//! are still partitioned and chunked by the regular batcher, but each chunk is
//! fed into an open Parquet writer for its partition instead of being held until
//! the whole object is in memory. See [`parquet_batched`] for details.

use vector_lib::codecs::encoding::ParquetSerializer;

use super::sink::S3RequestOptions;
use crate::sinks::{
    prelude::*,
    s3_common::{partitioner::S3KeyPartitioner, service::S3Metadata},
    util::parquet_batcher::{ParquetBatcherSettings, parquet_batched},
};

pub struct S3ParquetSink<Svc> {
    service: Svc,
    request_options: S3RequestOptions,
    partitioner: S3KeyPartitioner,
    batcher_settings: BatcherSettings,
    parquet_settings: ParquetBatcherSettings,
    serializer: ParquetSerializer,
    transformer: Transformer,
}

impl<Svc> S3ParquetSink<Svc> {
    pub const fn new(
        service: Svc,
        request_options: S3RequestOptions,
        partitioner: S3KeyPartitioner,
        batcher_settings: BatcherSettings,
        parquet_settings: ParquetBatcherSettings,
        serializer: ParquetSerializer,
        transformer: Transformer,
    ) -> Self {
        Self {
            service,
            request_options,
            partitioner,
            batcher_settings,
            parquet_settings,
            serializer,
            transformer,
        }
    }
}

#[async_trait]
impl<Svc> StreamSink<Event> for S3ParquetSink<Svc>
where
    Svc: Service<crate::sinks::s3_common::service::S3Request> + Send + 'static,
    Svc::Future: Send + 'static,
    Svc::Response: DriverResponse + Send + 'static,
    Svc::Error: std::fmt::Debug + Into<crate::Error> + Send,
{
    async fn run(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        let settings = self.batcher_settings;
        let request_options = self.request_options;

        let chunks = input
            .batched_partitioned(self.partitioner, settings.timeout, |_| {
                settings.as_byte_size_config()
            })
            .filter_map(|(key, batch)| async move { key.map(move |k| (k, batch)) });

        parquet_batched(
            chunks,
            self.serializer,
            self.transformer,
            self.parquet_settings,
        )
        .map(move |(partition_key, file)| {
            let (finalizers, metadata_builder, payload) = file.into_encode_result();
            let request_metadata = metadata_builder.build(&payload);
            let metadata = S3Metadata {
                s3_key: partition_key.key_prefix.clone(),
                partition_key,
                finalizers,
            };
            request_options.build_request(metadata, request_metadata, payload)
        })
        .into_driver(self.service)
        .run()
        .await
    }
}
