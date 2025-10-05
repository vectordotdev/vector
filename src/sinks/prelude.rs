//! Prelude module for sinks which will re-export the symbols that most
//! stream based sinks are likely to use.

pub use async_trait::async_trait;
pub use futures::{FutureExt, StreamExt, future, future::BoxFuture, stream::BoxStream};
pub use tower::{Service, ServiceBuilder};
pub use vector_lib::{
    ByteSizeOf, EstimatedJsonEncodedSizeOf,
    buffers::EventCount,
    config::{AcknowledgementsConfig, Input, telemetry},
    configurable::configurable_component,
    event::Value,
    finalization::{EventFinalizers, EventStatus, Finalizable},
    internal_event::{CountByteSize, TaggedEventsSent},
    json_size::JsonSize,
    partition::Partitioner,
    request_metadata::{GetEventCountTags, GroupedCountByteSize, MetaDescriptive, RequestMetadata},
    schema::Requirement,
    sink::{StreamSink, VectorSink},
    stream::{BatcherSettings, DriverResponse},
    tls::TlsSettings,
};

pub use crate::{
    codecs::{Encoder, EncodingConfig, Transformer},
    config::{DataType, GenerateConfig, SinkConfig, SinkContext},
    event::{Event, LogEvent},
    internal_events::{SinkRequestBuildError, TemplateRenderingError},
    sinks::{
        Healthcheck, HealthcheckError,
        util::{
            BatchConfig, Compression, Concurrency, NoDefaultsBatchSettings, RequestBuilder,
            SinkBatchSettings, TowerRequestConfig,
            builder::SinkBuilderExt,
            encoding::{self, write_all},
            metadata::RequestMetadataBuilder,
            request_builder::{EncodeResult, default_request_builder_concurrency_limit},
            retries::{RetryAction, RetryLogic},
            service::{ServiceBuilderExt, Svc},
        },
    },
    template::{Template, TemplateParseError, UnsignedIntTemplate},
    tls::TlsConfig,
};
