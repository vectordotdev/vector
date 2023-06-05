//! Prelude module for sinks which will re-export the symbols that most
//! stream based sinks are likely to use.

pub(crate) use crate::{
    codecs::{Encoder, EncodingConfig, Transformer},
    config::{DataType, GenerateConfig, SinkConfig, SinkContext},
    event::{Event, LogEvent},
    internal_events::TemplateRenderingError,
    sinks::util::retries::RetryLogic,
    sinks::{
        util::{
            builder::SinkBuilderExt,
            encoding::{self, write_all},
            metadata::RequestMetadataBuilder,
            request_builder::EncodeResult,
            service::{ServiceBuilderExt, Svc},
            BatchConfig, Compression, NoDefaultsBatchSettings, RequestBuilder, SinkBatchSettings,
            TowerRequestConfig,
        },
        Healthcheck,
    },
    template::{Template, TemplateParseError},
    tls::TlsConfig,
};
pub(crate) use async_trait::async_trait;
pub(crate) use futures::{future, future::BoxFuture, stream::BoxStream, FutureExt, StreamExt};
pub(crate) use tower::{Service, ServiceBuilder};
pub(crate) use vector_buffers::EventCount;
pub(crate) use vector_common::{
    finalization::{EventFinalizers, EventStatus, Finalizable},
    internal_event::CountByteSize,
    json_size::JsonSize,
    request_metadata::{MetaDescriptive, RequestMetadata},
};
pub(crate) use vector_config::configurable_component;
pub(crate) use vector_core::{
    config::{AcknowledgementsConfig, Input},
    event::Value,
    partition::Partitioner,
    schema::Requirement,
    sink::{StreamSink, VectorSink},
    stream::{BatcherSettings, DriverResponse},
    tls::TlsSettings,
    ByteSizeOf, EstimatedJsonEncodedSizeOf,
};
