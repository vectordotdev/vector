use std::marker::PhantomData;

use tower::ServiceBuilder;
use vector_config::configurable_component;
use vector_core::{
    config::{DataType, Input},
    sink::VectorSink,
    stream::BatcherSettings,
};

use crate::{
    aws::{AwsAuthentication, RegionOrEndpoint},
    codecs::{Encoder, EncodingConfig},
    config::AcknowledgementsConfig,
    sinks::util::{retries::RetryLogic, Compression, ServiceBuilderExt, TowerRequestConfig},
    tls::TlsConfig,
};

use super::{
    record::{Record, SendRecord},
    request_builder::KinesisRequestBuilder,
    sink::{BatchKinesisRequest, KinesisSink},
    KinesisResponse, KinesisService,
};

/// Base configuration for the `aws_kinesis_` sinks.
/// The actual specific sink configuration types should either wrap this in a newtype wrapper,
/// or should extend it in a new struct with `serde(flatten)`.
#[configurable_component]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct KinesisSinkBaseConfig {
    /// The [stream name][stream_name] of the target Kinesis Firehose delivery stream.
    ///
    /// [stream_name]: https://docs.aws.amazon.com/AmazonCloudWatch/latest/logs/Working-with-log-groups-and-streams.html
    #[configurable(metadata(docs::examples = "my-stream"))]
    pub stream_name: String,

    #[serde(flatten)]
    #[configurable(derived)]
    pub region: RegionOrEndpoint,

    #[configurable(derived)]
    pub encoding: EncodingConfig,

    #[configurable(derived)]
    #[serde(default)]
    pub compression: Compression,

    #[configurable(derived)]
    #[serde(default)]
    pub request: TowerRequestConfig,

    #[configurable(derived)]
    pub tls: Option<TlsConfig>,

    #[configurable(derived)]
    #[serde(default)]
    pub auth: AwsAuthentication,

    #[configurable(derived)]
    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::skip_serializing_if_default"
    )]
    pub acknowledgements: AcknowledgementsConfig,
}

impl KinesisSinkBaseConfig {
    pub fn input(&self) -> Input {
        Input::new(self.encoding.config().input_type() & DataType::Log)
    }

    pub const fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.acknowledgements
    }
}

/// Builds an aws_kinesis sink.
pub async fn build_sink<C, R, RR, E, RT>(
    config: &KinesisSinkBaseConfig,
    partition_key_field: Option<String>,
    batch_settings: BatcherSettings,
    client: C,
) -> crate::Result<VectorSink>
where
    C: SendRecord + Clone + Send + Sync + 'static,
    <C as SendRecord>::T: Send,
    <C as SendRecord>::E: Send + Sync + snafu::Error,
    Vec<<C as SendRecord>::T>: FromIterator<R>,
    R: Send + 'static,
    RR: Record + Record<T = R> + Clone + Send + Sync + Unpin + 'static,
    E: Send + 'static,
    RT: RetryLogic<Response = KinesisResponse> + Default,
{
    let request_limits = config.request.unwrap_with(&TowerRequestConfig::default());

    let region = config.region.region();
    let service = ServiceBuilder::new()
        .settings::<RT, BatchKinesisRequest<RR>>(request_limits, RT::default())
        .service(KinesisService::<C, R, E> {
            client,
            stream_name: config.stream_name.clone(),
            region,
            _phantom_t: PhantomData,
            _phantom_e: PhantomData,
        });

    let transformer = config.encoding.transformer();
    let serializer = config.encoding.build()?;
    let encoder = Encoder::<()>::new(serializer);

    let request_builder = KinesisRequestBuilder::<RR> {
        compression: config.compression,
        encoder: (transformer, encoder),
        _phantom: PhantomData,
    };

    let sink = KinesisSink {
        batch_settings,
        service,
        request_builder,
        partition_key_field,
        _phantom: PhantomData,
    };
    Ok(VectorSink::from_event_streamsink(sink))
}
