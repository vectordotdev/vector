use std::marker::PhantomData;
use vector_lib::lookup::lookup_v2::ConfigValuePath;

use vector_lib::stream::BatcherSettings;

use crate::{
    aws::{AwsAuthentication, RegionOrEndpoint},
    sinks::{
        prelude::*,
        util::{retries::RetryLogic, TowerRequestConfig},
    },
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

    /// Whether or not to retry successful requests containing partial failures.
    #[serde(default)]
    #[configurable(metadata(docs::advanced))]
    pub request_retry_partial: bool,

    #[configurable(derived)]
    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::is_default"
    )]
    pub acknowledgements: AcknowledgementsConfig,

    /// The log field used as the Kinesis record’s partition key value.
    ///
    /// If not specified, a unique partition key is generated for each Kinesis record.
    #[configurable(metadata(docs::examples = "user_id"))]
    pub partition_key_field: Option<ConfigValuePath>,
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
pub fn build_sink<C, R, RR, E, RT>(
    config: &KinesisSinkBaseConfig,
    partition_key_field: Option<ConfigValuePath>,
    batch_settings: BatcherSettings,
    client: C,
    retry_logic: RT,
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
    let request_limits = config.request.into_settings();

    let region = config.region.region();
    let service = ServiceBuilder::new()
        .settings::<RT, BatchKinesisRequest<RR>>(request_limits, retry_logic)
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
